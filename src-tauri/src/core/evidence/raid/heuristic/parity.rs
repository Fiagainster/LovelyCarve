//! Parity voting, occupancy samples and sample caching.
use super::*;

// ---------------------------------------------------------------------------
// RAID5 parity vote, occupancy, data-offset derivation
// ---------------------------------------------------------------------------

/// Sample dense stripe rows and return the fraction whose all-member XOR is
/// zero — the RAID5 parity vote. Sampling concentrates on the densely written
/// front of the device plus a spread across the whole disk; rows where fewer
/// than two members carry data are skipped (parity == data there is
/// uninformative). Returns `1.0` when no dense row is found at all (a wholly
/// empty group is inconclusive, so the RAID5 search still gets to try, gated by
/// filesystem validation). A single stale/dropped member depresses but does
/// not zero the ratio — its post-dropout rows break parity — so callers
/// threshold at [`PARITY_VOTE_MIN`] rather than requiring a perfect `1.0`.
pub(super) fn parity_vote(group: &[Arc<DiskImage>], size: u64) -> f64 {
    const BLOCK: usize = 4096;
    let offsets = parity_sample_offsets(size);
    let mut dense = 0u64;
    let mut zero = 0u64;
    for offset in offsets {
        let mut bufs = Vec::with_capacity(group.len());
        for image in group {
            match image.read_range(offset, BLOCK) {
                Ok(b) if b.len() == BLOCK => bufs.push(b),
                _ => {
                    bufs.clear();
                    break;
                }
            }
        }
        if bufs.len() != group.len() {
            continue;
        }
        let nonzero_members = bufs.iter().filter(|b| b.iter().any(|&x| x != 0)).count();
        if nonzero_members < 2 {
            continue;
        }
        dense += 1;
        let mut xor = vec![0u8; BLOCK];
        for b in &bufs {
            for (x, &y) in xor.iter_mut().zip(b.iter()) {
                *x ^= y;
            }
        }
        if xor.iter().all(|&x| x == 0) {
            zero += 1;
        }
    }
    if dense == 0 {
        return 1.0;
    }
    zero as f64 / dense as f64
}

/// 4 KiB-aligned offsets sampled for parity/occupancy scans: a dense sweep
/// over the written front (every 1 MiB across the first 512 MiB, where
/// filesystem metadata and small files live on a fresh install) plus 64 evenly
/// spaced rows across the remainder.
pub(super) fn parity_sample_offsets(size: u64) -> Vec<u64> {
    const BLOCK: u64 = 4096;
    let mut offsets = Vec::new();
    let front = size.min(512 * 1024 * 1024);
    let mut off = 0u64;
    while off + BLOCK <= front {
        offsets.push(off);
        off += 1024 * 1024;
    }
    if size > BLOCK {
        for i in 0..64u64 {
            let o = (size / 64) * i;
            offsets.push(o.min(size - BLOCK) & !0xFFF);
        }
    }
    offsets
}

/// Count of sampled non-zero 4 KiB blocks on a member — a cheap proxy for how
/// much data the disk carries. A dropped/stale RAID5 member stopped receiving
/// writes and is far emptier than its live peers.
pub(super) fn occupancy(image: &Arc<DiskImage>) -> u64 {
    const BLOCK: usize = 4096;
    let mut count = 0u64;
    for off in parity_sample_offsets(image.len()) {
        if let Ok(b) = image.read_range(off, BLOCK) {
            if b.iter().any(|&x| x != 0) {
                count += 1;
            }
        }
    }
    count
}

/// Identify a single strong low-occupancy outlier in the group — the likely
/// dropped/stale RAID5 member — to seed degraded reconstruction. Returns its
/// index when one disk is far emptier than the rest (< 0.3× the median of the
/// others, and the others are themselves non-trivially filled); `None`
/// otherwise (no clear outlier, or the group is uniformly sparse).
pub(super) fn stale_seed(group: &[Arc<DiskImage>]) -> Option<usize> {
    if group.len() < 3 {
        return None;
    }
    let occ: Vec<u64> = group.iter().map(occupancy).collect();
    let (min_idx, &min_occ) = occ.iter().enumerate().min_by_key(|(_, &v)| v)?;
    let mut others: Vec<u64> = occ
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != min_idx)
        .map(|(_, &v)| v)
        .collect();
    others.sort_unstable();
    let median = others[others.len() / 2];
    let nonzero_others = others.iter().filter(|&&v| v > 0).count();
    // Strong relative outlier (a partially-stale dropped disk, the real case),
    // or a fully-empty member among ≥2 filled peers (a blank/wiped column).
    let strong_outlier = median >= 8 && (min_occ as f64) < 0.3 * (median as f64);
    let empty_member = min_occ == 0 && nonzero_others >= 2;
    if strong_outlier || empty_member {
        Some(min_idx)
    } else {
        None
    }
}

/// Per-disk XOR-folded sample blocks, read once so subset ranking scores
/// candidates in memory instead of re-reading the disks for every subset. The
/// 64-bit fold is linear under XOR, so it preserves the RAID5 parity-zero
/// invariant exactly (no false negatives); rare fold collisions only add a
/// little noise to non-RAID5 subsets, which the exact reconstruction then
/// rejects anyway.
pub(super) struct SampleCache {
    /// `fold[disk][i]` = XOR-fold of the 4 KiB block at sample offset `i`
    /// (`0` for an all-zero or short block).
    pub(super) fold: Vec<Vec<u64>>,
    pub(super) occ: Vec<u64>,
}

impl SampleCache {
    pub(super) fn build(group: &[Arc<DiskImage>], size: u64) -> Self {
        const BLOCK: usize = 4096;
        let offsets = parity_sample_offsets(size);
        let mut fold = Vec::with_capacity(group.len());
        let mut occ = Vec::with_capacity(group.len());
        for image in group {
            let mut disk = Vec::with_capacity(offsets.len());
            let mut nonzero = 0u64;
            for &off in &offsets {
                let f = match image.read_range(off, BLOCK) {
                    Ok(b) if b.len() == BLOCK => fold64(&b),
                    _ => 0,
                };
                if f != 0 {
                    nonzero += 1;
                }
                disk.push(f);
            }
            fold.push(disk);
            occ.push(nonzero);
        }
        Self { fold, occ }
    }

    /// Parity vote over a subset of disk indices, on cached folds. Mirrors the
    /// row-by-row logic of [`parity_vote`].
    pub(super) fn parity_vote(&self, subset: &[usize]) -> f64 {
        let n_off = self.fold.first().map(Vec::len).unwrap_or(0);
        let mut dense = 0u64;
        let mut zero = 0u64;
        for i in 0..n_off {
            let nonzero_members = subset.iter().filter(|&&d| self.fold[d][i] != 0).count();
            if nonzero_members < 2 {
                continue;
            }
            dense += 1;
            let x = subset.iter().fold(0u64, |acc, &d| acc ^ self.fold[d][i]);
            if x == 0 {
                zero += 1;
            }
        }
        if dense == 0 {
            return 1.0;
        }
        zero as f64 / dense as f64
    }

    /// Stale-member detection within a subset (mirrors [`stale_seed`] on cached
    /// occupancy). Returns the position WITHIN `subset`, not the disk index.
    pub(super) fn subset_seed(&self, subset: &[usize]) -> Option<usize> {
        if subset.len() < 3 {
            return None;
        }
        let occ: Vec<u64> = subset.iter().map(|&d| self.occ[d]).collect();
        let (min_pos, &min_occ) = occ.iter().enumerate().min_by_key(|(_, &v)| v)?;
        let mut others: Vec<u64> = occ
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != min_pos)
            .map(|(_, &v)| v)
            .collect();
        others.sort_unstable();
        let median = others[others.len() / 2];
        let nonzero_others = others.iter().filter(|&&v| v > 0).count();
        if (median >= 8 && (min_occ as f64) < 0.3 * (median as f64))
            || (min_occ == 0 && nonzero_others >= 2)
        {
            Some(min_pos)
        } else {
            None
        }
    }
}

/// XOR-fold a block to a single 64-bit word (linear under XOR).
pub(super) fn fold64(b: &[u8]) -> u64 {
    let mut acc = 0u64;
    for chunk in b.chunks_exact(8) {
        acc ^= u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
    }
    acc
}
