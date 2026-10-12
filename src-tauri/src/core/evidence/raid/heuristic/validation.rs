//! Candidate validation, ranking and filesystem summaries.
use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct IntegrityScore {
    backup_match: bool,
    file_headers: u32,
}

/// Structural gate: does the assembled device begin with a partition table (or
/// bare filesystem) that contains at least one recognised filesystem and does
/// not claim space beyond the array? Returns the refined partitions on success.
pub(super) fn cheap_validate(
    source: &(dyn SectorSource + Send + Sync),
    capacity: u64,
) -> Option<(Vec<Partition>, TableScheme, u32)> {
    let detected = detect_partitions_with_meta(source, 512).ok()?;
    let scheme = detected.scheme?;
    let mut partitions = detected.partitions;
    if partitions.is_empty() {
        return None;
    }
    // A correct reconstruction's partitions fit exactly inside the capacity; a
    // table that runs past the assembled device is garbage from a wrong layout.
    if partitions
        .iter()
        .any(|p| (p.start_lba.saturating_add(p.length_lba)).saturating_mul(512) > capacity)
    {
        return None;
    }
    refine_partition_kinds(source, &mut partitions, 512).ok()?;
    let known = partitions.iter().filter(|p| is_filesystem(p.kind)).count() as u32;
    if known == 0 {
        return None;
    }
    let scheme_bonus = u32::from(matches!(scheme, TableScheme::Mbr | TableScheme::Gpt));
    Some((partitions, scheme, known + scheme_bonus))
}

pub(super) fn is_filesystem(kind: PartitionKind) -> bool {
    matches!(
        kind,
        PartitionKind::Ext4
            | PartitionKind::Btrfs
            | PartitionKind::Xfs
            | PartitionKind::Ntfs
            | PartitionKind::Refs
            | PartitionKind::Fat12
            | PartitionKind::Fat16
            | PartitionKind::Fat32
            | PartitionKind::ExFat
    )
}

/// Choose the best candidate. Filesystem deep validation (ext4 directory
/// descent or NTFS $MFT continuity + backup-boot anchor) is the decisive
/// signal; when no candidate exposes a deep-validatable filesystem, accept only
/// an unambiguous structural match so we never present a guessed-wrong
/// reconstruction. `seed` is the detected stale/dropped member (if any): among
/// otherwise-equal candidates the one that drops it is preferred, because its
/// post-dropout data is unreliable and must come from parity.
pub(super) fn pick_winner(
    mut survivors: Vec<Candidate>,
    seed: Option<usize>,
) -> Option<(Candidate, u32, usize)> {
    if survivors.is_empty() {
        return None;
    }
    survivors.sort_by_key(|c| Reverse(c.cheap_score));
    let structural_count = survivors.len();
    survivors.truncate(MAX_DEEP_VALIDATIONS);

    let mut best: Option<(Candidate, u32, usize)> = None;
    // Key: (passed, integrity, deep_score, prefer_drop_stale, cheap_score).
    // Threshold pass beats fail; backup anchors beat early-region-only aliases,
    // then ext4 file headers distinguish member order, then directory/MFT score;
    // then — at equal evidence — dropping the detected stale member; then the
    // structural score.
    let mut best_key = (0u8, IntegrityScore::default(), 0u32, 0u8, 0u32);
    for cand in survivors {
        let (deep, probed, passed, integrity) = deep_validate(&cand);
        tracing::trace!(level = cand.level_tag, layout = cand.layout_raw,
            chunk = cand.chunk_bytes, offset = cand.data_offset_bytes, order = ?cand.order,
            deep, probed, passed, ?integrity, "RAID candidate validation");
        let prefer_drop = match seed {
            Some(s) => u8::from(cand.missing_disk == Some(s)),
            None => 0,
        };
        let key = (passed as u8, integrity, deep, prefer_drop, cand.cheap_score);
        if best.is_none() || key > best_key {
            best_key = key;
            best = Some((cand, deep, probed));
        }
    }

    let (cand, deep, probed) = best?;
    tracing::debug!(level = cand.level_tag, layout = cand.layout_raw,
        chunk = cand.chunk_bytes, offset = cand.data_offset_bytes, order = ?cand.order,
        deep, probed, passed = best_key.0, integrity = ?best_key.1, structural_count, "RAID best candidate");
    if best_key.0 == 1 {
        return Some((cand, deep, probed));
    }
    // No filesystem verification possible: accept only when the structural
    // match was unique, otherwise the sparse-array ambiguity makes any pick a
    // guess.
    if structural_count == 1 && cand.cheap_score >= 2 {
        return Some((cand, deep, probed));
    }
    None
}

/// Deep-validate a candidate against the largest filesystem it exposes.
/// Returns `(score, probed, passed, integrity)` where `passed` means a
/// filesystem cleared its accept threshold and `integrity` ranks distributed
/// backup anchors followed by matching ext4 file headers. These distinguish
/// layouts that only happen to reproduce the early directory metadata.
///
/// Three filesystem families are checked, on bare partitions and — when a bare
/// filesystem first looks consistent — on the logical volumes inside an LVM
/// partition (the Linux server shape: a small ext4/XFS `/boot` beside an LVM PV
/// holding the real root filesystem). ext4 uses directory descent; NTFS uses
/// $MFT continuity; XFS uses directory-descent consistency.
pub(super) fn deep_validate(cand: &Candidate) -> (u32, usize, bool, IntegrityScore) {
    // Best (score, probed, passed, integrity) seen so far, ranked by
    // (passed, integrity, score). Updated in place as each filesystem is scored.
    let mut best = (0u32, 0usize, false, IntegrityScore::default());

    // 1. Bare partitions. Track whether any bare filesystem looks consistent so
    //    far, to gate the (4 MiB) LVM open below.
    let mut lvm_part: Option<&Partition> = None;
    let mut bare_promising = false;
    for part in &cand.partitions {
        let base = part.start_lba * 512;
        let len = part.length_lba * 512;
        match part.kind {
            PartitionKind::Ext4 => {
                let (score, probed, file_headers) =
                    ext4_descend_score(cand.source.clone(), base, len);
                let integrity = IntegrityScore {
                    backup_match: ext4_backups_match(cand.source.as_ref(), base, len),
                    file_headers,
                };
                if score >= 2 {
                    bare_promising = true;
                }
                accumulate_deep(
                    &mut best,
                    score,
                    probed,
                    score >= MIN_DESCEND_SCORE,
                    integrity,
                );
            }
            PartitionKind::Ntfs => {
                let (score, probed) = ntfs_mft_score(cand.source.clone(), base, len);
                let integrity = IntegrityScore {
                    backup_match: ntfs_backup_matches(cand.source.as_ref(), base, len),
                    file_headers: 0,
                };
                if score >= 2 {
                    bare_promising = true;
                }
                accumulate_deep(&mut best, score, probed, score >= MIN_MFT_SCORE, integrity);
            }
            PartitionKind::Xfs => {
                let d = xfs_descend_score(cand.source.as_ref(), base, len);
                if d.promising() {
                    bare_promising = true;
                }
                accumulate_deep(
                    &mut best,
                    d.clean_dirs,
                    d.probed,
                    d.passed(),
                    IntegrityScore::default(),
                );
            }
            PartitionKind::Lvm => lvm_part = Some(part),
            _ => {}
        }
    }

    // 2. LVM logical volumes. Only opened when a bare filesystem already looks
    //    consistent — a wrong RAID layout corrupts even the small `/boot` tree
    //    within a couple of directories, so this gate keeps the expensive LV
    //    descent off the thousands of wrong candidates while still reaching it
    //    for the right one (and the handful that fake a readable `/boot` front).
    if let (Some(part), true) = (lvm_part, bare_promising) {
        let part_off = part.start_lba * 512;
        let part_len = part.length_lba * 512;
        for lv in lv_regions_from_source(cand.source.as_ref(), part_off, part_len) {
            let base = part_off + lv.offset;
            match lv.kind {
                PartitionKind::Xfs => {
                    let d = xfs_descend_score(cand.source.as_ref(), base, lv.size_bytes);
                    accumulate_deep(
                        &mut best,
                        d.clean_dirs,
                        d.probed,
                        d.passed(),
                        IntegrityScore::default(),
                    );
                }
                PartitionKind::Ext4 => {
                    let (score, probed, file_headers) =
                        ext4_descend_score(cand.source.clone(), base, lv.size_bytes);
                    let integrity = IntegrityScore {
                        backup_match: ext4_backups_match(cand.source.as_ref(), base, lv.size_bytes),
                        file_headers,
                    };
                    accumulate_deep(
                        &mut best,
                        score,
                        probed,
                        score >= MIN_DESCEND_SCORE,
                        integrity,
                    );
                }
                _ => {}
            }
        }
    }

    best
}

/// Keep the stronger of two deep-validation results in `best`, ranked by
/// (passed, integrity, score) — the same precedence `pick_winner` uses.
pub(super) fn accumulate_deep(
    best: &mut (u32, usize, bool, IntegrityScore),
    score: u32,
    probed: usize,
    passed: bool,
    integrity: IntegrityScore,
) {
    let cur = (u8::from(passed), integrity, score);
    let prev = (u8::from(best.2), best.3, best.0);
    if cur > prev {
        *best = (score, probed, passed, integrity);
    }
}

/// True when the NTFS volume's backup boot sector (at the last volume sector,
/// `total_sectors`) reproduces the primary VBR's OEM + BPB + serial. NTFS
/// mirrors its boot sector at the very end of the volume, so a correct
/// reconstruction matches it across the **entire** span — a wrong layout that
/// only aligns the early metadata region does not. Bounds-checked; `false` on
/// any short/garbage read.
pub(super) fn ntfs_backup_matches(
    source: &(dyn SectorSource + Send + Sync),
    base: u64,
    len: u64,
) -> bool {
    let Some(primary) = read_rel(source, base, len, 0, 512) else {
        return false;
    };
    if primary.get(3..11) != Some(b"NTFS    ".as_slice()) {
        return false;
    }
    let bps = match le16(&primary, 0x0B) {
        Some(v) if v >= 512 => v as u64,
        _ => return false,
    };
    let total_sectors = u64::from_le_bytes(primary[0x28..0x30].try_into().unwrap_or([0; 8]));
    let Some(backup_off) = total_sectors.checked_mul(bps) else {
        return false;
    };
    let Some(backup) = read_rel(source, base, len, backup_off, 512) else {
        return false;
    };
    // Compare OEM + BPB + volume serial; tolerate any boot-code/trailer noise.
    primary.get(3..0x50) == backup.get(3..0x50)
}

/// Largest partition carrying a filesystem we can deep-validate (ext4 / NTFS).
/// Largest partition carrying a filesystem we can summarise for the array
/// label / `RaidFilesystem` record (ext4 / NTFS / XFS).
pub(super) fn largest_validatable_fs(partitions: &[Partition]) -> Option<&Partition> {
    partitions
        .iter()
        .filter(|p| {
            matches!(
                p.kind,
                PartitionKind::Ext4 | PartitionKind::Ntfs | PartitionKind::Xfs
            )
        })
        .max_by_key(|p| p.length_lba)
}

/// Pull UUID / size / block size straight from the ext4 superblock at
/// `part_offset + 1024` for the [`RaidFilesystem`] record and a stable
/// `set_uuid`.
pub(super) fn read_ext4_sb_info(
    source: &(dyn SectorSource + Send + Sync),
    part_offset: u64,
) -> Option<Ext4SbInfo> {
    let sb = source.read(part_offset + 1024, 0x100).ok()?;
    if sb.len() < 0x100 || sb[0x38] != 0x53 || sb[0x39] != 0xEF {
        return None;
    }
    let blocks_lo = u32::from_le_bytes(sb[0x04..0x08].try_into().ok()?) as u64;
    let log_block_size = u32::from_le_bytes(sb[0x18..0x1C].try_into().ok()?);
    let block_size = 1024u32 << log_block_size.min(20);
    let uuid_hex = sb[0x68..0x78].iter().map(|b| format!("{b:02x}")).collect();
    Some(Ext4SbInfo {
        uuid_hex,
        total_bytes: blocks_lo.saturating_mul(block_size as u64),
        block_size,
    })
}

/// Read a normalised [`FsSummary`] for a partition, dispatching on its kind.
pub(super) fn read_fs_summary(
    source: &(dyn SectorSource + Send + Sync),
    part: &Partition,
) -> Option<FsSummary> {
    let base = part.start_lba * 512;
    match part.kind {
        PartitionKind::Ext4 => {
            let info = read_ext4_sb_info(source, base)?;
            Some(FsSummary {
                kind: "ext4",
                fsid: Some(info.uuid_hex),
                total_bytes: Some(info.total_bytes),
                sector_size: Some(info.block_size),
            })
        }
        PartitionKind::Ntfs => {
            let info = read_ntfs_sb_info(source, base)?;
            Some(FsSummary {
                kind: "ntfs",
                fsid: Some(info.serial_hex),
                total_bytes: Some(info.total_bytes),
                sector_size: Some(info.bytes_per_sector),
            })
        }
        PartitionKind::Xfs => {
            let info = read_xfs_sb_info(source, base)?;
            Some(FsSummary {
                kind: "xfs",
                fsid: Some(info.uuid_hex),
                total_bytes: Some(info.total_bytes),
                sector_size: Some(info.block_size),
            })
        }
        _ => None,
    }
}

/// Pull UUID / size / block size from the XFS superblock at `part_offset` (all
/// big-endian) for the [`RaidFilesystem`] record and a stable `set_uuid`.
pub(super) fn read_xfs_sb_info(
    source: &(dyn SectorSource + Send + Sync),
    part_offset: u64,
) -> Option<XfsSbInfo> {
    let sb = source.read(part_offset, 512).ok()?;
    if sb.len() < 0x70 || sb.get(0..4)? != [0x58, 0x46, 0x53, 0x42] {
        return None;
    }
    let block_size = u32::from_be_bytes(sb[0x04..0x08].try_into().ok()?);
    let dblocks = u64::from_be_bytes(sb[0x08..0x10].try_into().ok()?);
    let uuid_hex = sb[0x20..0x30].iter().map(|b| format!("{b:02x}")).collect();
    Some(XfsSbInfo {
        uuid_hex,
        total_bytes: dblocks.saturating_mul(block_size as u64),
        block_size,
    })
}

/// Pull volume serial / size / sector size from the NTFS boot sector at
/// `part_offset` for the [`RaidFilesystem`] record and a stable `set_uuid`.
pub(super) fn read_ntfs_sb_info(
    source: &(dyn SectorSource + Send + Sync),
    part_offset: u64,
) -> Option<NtfsSbInfo> {
    let boot = source.read(part_offset, 512).ok()?;
    if boot.len() < 0x50 || boot.get(3..11)? != b"NTFS    " {
        return None;
    }
    let bytes_per_sector = u16::from_le_bytes(boot[0x0B..0x0D].try_into().ok()?) as u64;
    let total_sectors = u64::from_le_bytes(boot[0x28..0x30].try_into().ok()?);
    let serial = u64::from_le_bytes(boot[0x48..0x50].try_into().ok()?);
    Some(NtfsSbInfo {
        serial_hex: format!("{serial:016x}"),
        total_bytes: total_sectors.saturating_mul(bytes_per_sector),
        bytes_per_sector: bytes_per_sector as u32,
    })
}
