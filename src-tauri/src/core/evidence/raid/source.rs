//! Virtual block-device readers used during RAID assembly.
//!
//! Three readers live here, all implementing `SectorSource`:
//!
//! - `LinearLvSource` — concatenates byte ranges from one or more
//!   `Arc<DiskImage>` handles into a single logical address space. Used to
//!   replay an LVM linear LV that may span multiple physical images.
//! - `WholeDiskMember` — exposes a raw byte range of an `Arc<DiskImage>` as
//!   a `SectorSource`. Used when an mdadm member is a physical partition
//!   (or whole disk) rather than an LV.
//! - `RaidVolume` — striped reader covering both RAID0 and RAID5:
//!   one logical type, parameterised by a `RaidLevel` enum that carries
//!   the RAID5 rotation. Reads dispatch to the canonical Linux mdadm
//!   chunk-striping math (`stripe_step` / `raid5_step`). RAID5 supports
//!   layouts 0..3 (left/right × symmetric/asymmetric); RAID0 has only
//!   one layout.
//!
//! Keeping all three together here means the orchestration in `raid.rs`
//! never imports anything from this file other than the public `set` /
//! `read` surface, which lets each reader evolve independently of the
//! detection pass.

use std::sync::Arc;

use crate::core::evidence::image::DiskImage;
use crate::core::evidence::partitions::SectorSource;

/// Erased reader. Every RAID member reads through one of these so the
/// striping math stays the same regardless of whether the member is an
/// LV, a partition slice, or some future construct.
pub type MemberReader = Arc<dyn SectorSource + Send + Sync>;

/// Virtual block device for one LV that may span multiple PVs across
/// multiple physical images. Implements `SectorSource` so the rest of the
/// stack (mdadm probe, RAID0/5 striper, synthetic `DiskImage`) talks to
/// it through the same trait used by raw mmap / EWF / VMDK readers.
#[derive(Clone)]
pub struct LinearLvSource {
    pub vg_uuid: String,
    pub vg_name: String,
    pub lv_name: String,
    pub size_bytes: u64,
    /// Ordered list of byte ranges that, concatenated, form the LV's
    /// logical address space. Each segment carries the `Arc<DiskImage>` it
    /// reads from so dispatch is a single Arc clone away.
    pub segments: Vec<LvByteSegment>,
}

#[derive(Clone)]
pub struct LvByteSegment {
    pub image: Arc<DiskImage>,
    pub byte_offset: u64,
    pub byte_length: u64,
}

impl LinearLvSource {
    /// Inherent reader; named `read_at` to leave the unqualified `read`
    /// identifier free for the `SectorSource` trait impl below.
    pub fn read_at(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if len == 0 || offset >= self.size_bytes {
            return Ok(Vec::new());
        }
        let mut remaining = (self.size_bytes - offset).min(len as u64) as usize;
        let mut cursor = offset;
        let mut out = Vec::with_capacity(remaining);
        let mut seg_start = 0u64;
        for segment in &self.segments {
            let seg_end = seg_start
                .checked_add(segment.byte_length)
                .ok_or_else(|| anyhow::anyhow!("LVM segment boundary overflow"))?;
            if cursor < seg_end {
                let local_off = cursor - seg_start;
                let take = (segment.byte_length - local_off).min(remaining as u64) as usize;
                if take == 0 {
                    break;
                }
                let bytes = segment.image.read_range(
                    segment
                        .byte_offset
                        .checked_add(local_off)
                        .ok_or_else(|| anyhow::anyhow!("LVM source offset overflow"))?,
                    take,
                )?;
                if bytes.len() != take {
                    anyhow::bail!("short LVM allocated segment read");
                }
                out.extend_from_slice(&bytes);
                cursor += bytes.len() as u64;
                remaining -= bytes.len();
                if remaining == 0 {
                    break;
                }
            }
            seg_start = seg_end;
            if seg_start >= self.size_bytes {
                break;
            }
        }
        if remaining != 0 {
            anyhow::bail!("incomplete LVM segment map");
        }
        Ok(out)
    }
}

impl SectorSource for LinearLvSource {
    fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        self.read_at(offset, len)
    }
    fn size(&self) -> u64 {
        self.size_bytes
    }
}

/// Whole-disk (or whole-partition) byte-range adapter that lets a raw
/// `DiskImage` slice participate in RAID assembly through the same
/// `SectorSource` trait used for LV-backed members.
pub struct WholeDiskMember {
    pub image: Arc<DiskImage>,
    pub byte_offset: u64,
    pub byte_length: u64,
}

impl SectorSource for WholeDiskMember {
    fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if offset >= self.byte_length || len == 0 {
            return Ok(Vec::new());
        }
        let take = (self.byte_length - offset).min(len as u64) as usize;
        self.image.read_range(self.byte_offset + offset, take)
    }
    fn size(&self) -> u64 {
        self.byte_length
    }
}

/// One member in a striped array. Slot index defines its position in the
/// RAID layout (slot 0 .. slot raid_disks-1); the array's striping math
/// indexes `members` by slot. `data_offset_bytes` is where the data area
/// starts on this member (mdadm v1.2 typically reserves a header region
/// before stripe data); `data_size_bytes` is the available data length.
#[derive(Clone)]
pub struct StripedMember {
    pub reader: MemberReader,
    pub data_offset_bytes: u64,
    pub data_size_bytes: u64,
}

/// Virtual RAID0/RAID5 reader. Single type with a `RaidLevel` discriminant.
///
/// `members` is slot-indexed: `members[i]` is the member occupying RAID
/// slot `i` for `i ∈ 0..raid_disks`. The caller establishes that ordering
/// via `MdadmSuperblock::slot_index` — `dev_number` alone is insufficient
/// because mdadm reuses dev_numbers across array events (see
/// `服务器部分/server3.001`, dev_number=3 occupying slot 2).
///
/// RAID5 reads only the data portion of each stripe row (the parity column
/// is skipped). A **single** missing/stale column can be reconstructed on the
/// fly via `missing_slot` (degraded read): every physical chunk of the absent
/// member is rebuilt by XOR-ing the other `n-1` members at the same
/// `member_offset`. With `missing_slot == None` assembly only succeeds when
/// every data slot is present; a missing parity disk needs no reconstruction
/// (its chunks are never read). RAID6 double-failure recovery is still out of
/// scope.
pub struct RaidVolume {
    level: RaidLevel,
    sector_size: u32,
    members: Vec<StripedMember>,
    chunk_bytes: u64,
    /// Logical capacity reported to the FS layer. Set to `None` to derive
    /// from member sizes (`level.capacity_bytes(members)`); set to
    /// `Some(n)` to honour the SB's recorded `size_sectors`. When present the
    /// caller has authoritative information from the superblock; when
    /// absent we fall back to the kernel-style minimum-member computation.
    logical_size_bytes: Option<u64>,
    /// Slot index (0..members.len()-1) of a member whose physical data is
    /// missing or stale (a dropped RAID5 disk). When a read lands on this
    /// slot the chunk is reconstructed by XOR over the other members rather
    /// than read from `members[missing_slot]` (whose reader may still be
    /// present but carries pre-dropout data that must be ignored). Only
    /// meaningful for `RaidLevel::Raid5`; ignored for RAID0.
    missing_slot: Option<usize>,
}

/// RAID level + RAID5 rotation as a single discriminant.
/// RAID6 is not yet implemented; RAID1 surfaces
/// descriptively only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidLevel {
    /// Striped without parity. No rotation.
    Raid0,
    /// Striped with one parity disk. The wrapped `Raid5Layout` is the
    /// kernel `layout` field (0..3 = left/right × asymmetric/symmetric).
    Raid5(Raid5Layout),
}

impl RaidLevel {
    /// Build a level from the superblock's raw `level` + `layout`. Returns
    /// `None` for unsupported levels (RAID1 / RAID6 / RAID10 / linear) so
    /// the caller can downgrade to a descriptive-only `RaidArray` record.
    pub fn from_raw(level: i32, layout: u32) -> Option<Self> {
        match level {
            0 => Some(Self::Raid0),
            5 => Raid5Layout::from_raw(layout).map(Self::Raid5),
            _ => None,
        }
    }

    /// Per-member capacity → logical-disk capacity for this level.
    /// RAID0: sum of all members. RAID5: `(n-1)` data members times the
    /// smallest member's size (mdadm guarantees equal sizes; smallest is
    /// the safe choice when one member is truncated).
    pub fn capacity_bytes(&self, members: &[StripedMember]) -> u64 {
        match self {
            Self::Raid0 => members.iter().map(|m| m.data_size_bytes).sum(),
            Self::Raid5(_) => {
                let n = members.len() as u64;
                if n < 2 {
                    return 0;
                }
                let per_member = members.iter().map(|m| m.data_size_bytes).min().unwrap_or(0);
                (n - 1) * per_member
            }
        }
    }
}

impl RaidVolume {
    /// Construct a virtual RAID0/RAID5 device. `level` packs type and rotation,
    /// `chunk_bytes` is the stripe size in bytes, and per-member start
    /// sectors live on `StripedMember` rather than the array itself
    /// (allowing per-member data offsets, which mdadm v1.x supports
    /// even though `mdadm --create` always writes them uniformly).
    ///
    /// `logical_size_bytes`: `Some(n)` to honour the SB's recorded array
    /// size; `None` to derive from members. `RaidVolume::size()` returns
    /// the explicit value when set, the derived value otherwise.
    pub fn set(
        level: RaidLevel,
        sector_size: u32,
        chunk_bytes: u64,
        logical_size_bytes: Option<u64>,
        members: Vec<StripedMember>,
    ) -> Self {
        Self::set_degraded(
            level,
            sector_size,
            chunk_bytes,
            logical_size_bytes,
            members,
            None,
        )
    }

    /// Like [`set`](Self::set) but marks one slot as missing/stale so reads
    /// landing on it are reconstructed by XOR over the remaining members
    /// (degraded RAID5 read). `missing_slot` must be `< members.len()`;
    /// out-of-range values are ignored (treated as `None`). For RAID0 the
    /// flag is meaningless and never consulted.
    pub fn set_degraded(
        level: RaidLevel,
        sector_size: u32,
        chunk_bytes: u64,
        logical_size_bytes: Option<u64>,
        members: Vec<StripedMember>,
        missing_slot: Option<usize>,
    ) -> Self {
        let missing_slot = missing_slot.filter(|&s| s < members.len());
        Self {
            level,
            sector_size,
            members,
            chunk_bytes,
            logical_size_bytes,
            missing_slot,
        }
    }

    /// Slot index of the reconstructed (missing/stale) member, if any.
    pub fn missing_slot(&self) -> Option<usize> {
        self.missing_slot
    }

    pub fn level(&self) -> RaidLevel {
        self.level
    }

    pub fn sector_size(&self) -> u32 {
        self.sector_size
    }

    pub fn chunk_bytes(&self) -> u64 {
        self.chunk_bytes
    }

    pub fn members(&self) -> &[StripedMember] {
        &self.members
    }

    /// Total logical bytes addressable through this volume. Honours the
    /// `logical_size_bytes` override when set; falls back to the
    /// level-derived capacity otherwise.
    pub fn total_bytes(&self) -> u64 {
        self.logical_size_bytes
            .unwrap_or_else(|| self.level.capacity_bytes(&self.members))
    }

    /// Inherent reader; named `read_at` to leave `read` for the
    /// `SectorSource` trait impl.
    pub fn read_at(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if self.members.is_empty() || self.chunk_bytes == 0 || len == 0 {
            return Ok(Vec::new());
        }
        let total = self.total_bytes();
        if offset >= total {
            return Ok(Vec::new());
        }
        // RAID5 needs at least 2 members for the layout math to be
        // defined (`n - 1` data disks). RAID0 with 1 member is degenerate
        // but legal — passes through to the single member.
        if matches!(self.level, RaidLevel::Raid5(_)) && self.members.len() < 2 {
            return Ok(Vec::new());
        }
        let mut remaining = (total - offset).min(len as u64) as usize;
        let mut out = Vec::with_capacity(remaining);
        let mut cursor = offset;
        let n = self.members.len() as u64;
        while remaining > 0 {
            let step = self.step(cursor, n, remaining);
            let bytes = self.read_step(&step)?;
            if bytes.is_empty() {
                break;
            }
            let appended = bytes.len();
            out.extend_from_slice(&bytes);
            cursor += appended as u64;
            remaining = remaining.saturating_sub(appended);
            if appended < step.take_len {
                break;
            }
        }
        Ok(out)
    }

    /// Resolve one stripe step to bytes. A normal step reads its member
    /// directly; a step landing on `missing_slot` is reconstructed by XOR
    /// over the other members at the same physical `member_offset`.
    fn read_step(&self, step: &StripeStep) -> anyhow::Result<Vec<u8>> {
        if self.missing_slot == Some(step.member_index) {
            return self.reconstruct_chunk(step.member_index, step.member_offset, step.take_len);
        }
        let member = &self.members[step.member_index];
        if step.member_offset >= member.data_size_bytes {
            return Ok(Vec::new());
        }
        let take = step
            .take_len
            .min((member.data_size_bytes - step.member_offset) as usize);
        let phys_offset = member.data_offset_bytes + step.member_offset;
        member.reader.read(phys_offset, take)
    }

    /// Rebuild a missing member's chunk at `member_offset` (length `want`) by
    /// XOR-ing the other members at the same offset. Correct for any RAID5
    /// layout: at a given stripe row the `n` physical chunks satisfy
    /// `XOR(all n) == 0` (parity is the XOR of the data chunks), so the
    /// absent chunk equals the XOR of the surviving `n-1` — independent of
    /// whether it would have held data or parity. Short reads (end of a
    /// member) are zero-extended implicitly; the result is truncated to the
    /// longest peer read so the caller's boundary detection still fires.
    fn reconstruct_chunk(
        &self,
        missing: usize,
        member_offset: u64,
        want: usize,
    ) -> anyhow::Result<Vec<u8>> {
        let mut acc = vec![0u8; want];
        let mut max_valid = 0usize;
        for (i, member) in self.members.iter().enumerate() {
            if i == missing || member_offset >= member.data_size_bytes {
                continue;
            }
            let take = want.min((member.data_size_bytes - member_offset) as usize);
            let phys = member.data_offset_bytes + member_offset;
            let bytes = member.reader.read(phys, take)?;
            for (a, &b) in acc.iter_mut().zip(bytes.iter()) {
                *a ^= b;
            }
            max_valid = max_valid.max(bytes.len());
        }
        acc.truncate(max_valid);
        Ok(acc)
    }

    fn step(&self, cursor: u64, n: u64, remaining: usize) -> StripeStep {
        match self.level {
            RaidLevel::Raid0 => stripe_step(cursor, self.chunk_bytes, n, remaining),
            RaidLevel::Raid5(layout) => {
                let r = raid5_step(cursor, self.chunk_bytes, n, layout, remaining);
                StripeStep {
                    member_index: r.member_index,
                    member_offset: r.member_offset,
                    take_len: r.take_len,
                }
            }
        }
    }
}

impl SectorSource for RaidVolume {
    fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        self.read_at(offset, len)
    }
    fn size(&self) -> u64 {
        self.total_bytes()
    }
}

/// One step of the RAID0 striping math, factored out so it can be
/// exercised independently of the `RaidVolume` plumbing. Mirrors the
/// canonical Linux mdadm RAID0 layout: chunk N goes to member `N % n`,
/// and the in-member offset is `(chunk_index / n) * chunk_bytes + offset_within_chunk`.
pub(crate) struct StripeStep {
    pub member_index: usize,
    pub member_offset: u64,
    pub take_len: usize,
}

pub(crate) fn stripe_step(cursor: u64, chunk_bytes: u64, n: u64, remaining: usize) -> StripeStep {
    let chunk_index = cursor / chunk_bytes;
    let in_chunk = cursor % chunk_bytes;
    let member_index = (chunk_index % n) as usize;
    let stripe_round = chunk_index / n;
    let member_offset = stripe_round * chunk_bytes + in_chunk;
    let take_len = (chunk_bytes - in_chunk).min(remaining as u64) as usize;
    StripeStep {
        member_index,
        member_offset,
        take_len,
    }
}

/// Linux mdadm RAID5 layout encoding (`layout` field of v1.x SB).
///
/// 0/1: parity disk advances; data fills the remaining slots in disk-index
/// order. 2/3: parity disk advances; data starts immediately to the right
/// of parity and wraps. "Left" rotates parity backward (right-to-left as
/// stripe row advances); "right" rotates parity forward.
///
/// `2` (LEFT_SYMMETRIC) is the kernel default and the layout reported by
/// stock `mdadm --create` without an explicit `--layout`. The test images
/// in `服务器部分/server*.001` use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raid5Layout {
    LeftAsymmetric,
    RightAsymmetric,
    LeftSymmetric,
    RightSymmetric,
}

impl Raid5Layout {
    pub fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::LeftAsymmetric),
            1 => Some(Self::RightAsymmetric),
            2 => Some(Self::LeftSymmetric),
            3 => Some(Self::RightSymmetric),
            _ => None,
        }
    }

    /// Lowercase ASCII label used in tracing output.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LeftAsymmetric => "left-asymmetric",
            Self::RightAsymmetric => "right-asymmetric",
            Self::LeftSymmetric => "left-symmetric",
            Self::RightSymmetric => "right-symmetric",
        }
    }
}

/// One step of the RAID5 striping math. `n` is `raid_disks`; data_disks =
/// `n - 1`. The math is fully derived from the canonical kernel routines
/// in `drivers/md/raid5.c::raid5_compute_sector` for layouts 0..3 of
/// RAID5/4 (parity rotates by stripe row, data fills the remaining
/// columns).
pub(crate) struct Raid5Step {
    pub member_index: usize,
    pub member_offset: u64,
    pub take_len: usize,
}

pub(crate) fn raid5_step(
    cursor: u64,
    chunk_bytes: u64,
    n: u64,
    layout: Raid5Layout,
    remaining: usize,
) -> Raid5Step {
    let data_disks = n - 1;
    // Walk the **logical** address space in chunks of size `chunk_bytes`.
    // Each `data_disks` consecutive logical chunks form one stripe row.
    let logical_chunk = cursor / chunk_bytes;
    let in_chunk = cursor % chunk_bytes;
    let stripe_row = logical_chunk / data_disks;
    let data_pos = (logical_chunk % data_disks) as u32; // 0..data_disks-1

    let n_u32 = n as u32;
    let parity_disk = match layout {
        Raid5Layout::LeftAsymmetric | Raid5Layout::LeftSymmetric => {
            // Parity rotates from slot (n-1) toward slot 0 as stripe_row
            // advances; equivalent kernel form: data_disks - (stripe % n).
            ((n_u32 - 1) - (stripe_row % n) as u32) % n_u32
        }
        Raid5Layout::RightAsymmetric | Raid5Layout::RightSymmetric => {
            (stripe_row % n) as u32 % n_u32
        }
    };

    let physical_disk = match layout {
        Raid5Layout::LeftAsymmetric | Raid5Layout::RightAsymmetric => {
            // Asymmetric: data fills slots in disk-index order, skipping
            // parity. Slot i (0..data_disks) lives on disk i if i < parity,
            // else on disk i+1.
            if data_pos < parity_disk {
                data_pos
            } else {
                data_pos + 1
            }
        }
        Raid5Layout::LeftSymmetric | Raid5Layout::RightSymmetric => {
            // Symmetric: data starts immediately to the right of parity
            // and wraps, so the i-th data slot lives on (parity + 1 + i) % n.
            (parity_disk + 1 + data_pos) % n_u32
        }
    };

    let take_len = (chunk_bytes - in_chunk).min(remaining as u64) as usize;
    Raid5Step {
        member_index: physical_disk as usize,
        member_offset: stripe_row * chunk_bytes + in_chunk,
        take_len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Striping math: with chunk size 4 and 2 members, logical bytes 0..3
    /// land on member 0 chunk 0; bytes 4..7 land on member 1 chunk 0;
    /// bytes 8..11 land on member 0 chunk 1; bytes 12..15 land on member
    /// 1 chunk 1. The in-member offset for the second visit to a member
    /// is `chunk_bytes` (one stripe round forward).
    #[test]
    fn stripe_step_walks_canonical_raid0_layout() {
        let chunk = 4u64;
        let n = 2u64;

        let step = stripe_step(0, chunk, n, 16);
        assert_eq!(step.member_index, 0);
        assert_eq!(step.member_offset, 0);
        assert_eq!(step.take_len, 4);

        let step = stripe_step(4, chunk, n, 12);
        assert_eq!(step.member_index, 1);
        assert_eq!(step.member_offset, 0);

        let step = stripe_step(8, chunk, n, 8);
        assert_eq!(step.member_index, 0);
        assert_eq!(step.member_offset, 4);

        let step = stripe_step(12, chunk, n, 4);
        assert_eq!(step.member_index, 1);
        assert_eq!(step.member_offset, 4);
    }

    /// Mid-chunk reads: cursor 2 with chunk size 4 gives in-chunk offset 2
    /// and a clamped take_len of `chunk - in_chunk`.
    #[test]
    fn stripe_step_clamps_take_to_chunk_boundary() {
        let step = stripe_step(2, 4, 2, 100);
        assert_eq!(step.member_index, 0);
        assert_eq!(step.member_offset, 2);
        assert_eq!(step.take_len, 2);
    }

    /// 3-disk RAID0: chunk N goes to member `N % 3`, in-member offset
    /// advances by `chunk_bytes` every full stripe round.
    #[test]
    fn stripe_step_handles_three_member_array() {
        for chunk_index in 0..9u64 {
            let cursor = chunk_index * 8;
            let step = stripe_step(cursor, 8, 3, 8);
            assert_eq!(step.member_index, (chunk_index % 3) as usize);
            assert_eq!(step.member_offset, (chunk_index / 3) * 8);
        }
    }

    /// 3-disk RAID5 left-symmetric (the kernel default and the layout used
    /// by the `服务器部分/server*.001` test images). Verifies the
    /// disk-mapping table from the case report:
    ///   stripe 0: parity=2, data → disks 0, 1
    ///   stripe 1: parity=1, data → disks 2, 0
    ///   stripe 2: parity=0, data → disks 1, 2
    ///   stripe 3: parity=2 (cycle), data → disks 0, 1
    #[test]
    fn raid5_left_symmetric_three_disk_layout() {
        let chunk = 8u64;
        let n = 3u64;
        let expected = [
            (0u64, 0usize, 0u64), // logical chunk 0 → disk 0, member offset 0
            (1, 1, 0),            // logical chunk 1 → disk 1, member offset 0
            (2, 2, 8),            // logical chunk 2 → disk 2, member offset 8 (stripe row 1)
            (3, 0, 8),            // logical chunk 3 → disk 0, member offset 8
            (4, 1, 16),           // logical chunk 4 → disk 1, member offset 16 (stripe row 2)
            (5, 2, 16),           // logical chunk 5 → disk 2, member offset 16
            (6, 0, 24), // logical chunk 6 → disk 0, member offset 24 (stripe row 3 cycles back)
            (7, 1, 24), // logical chunk 7 → disk 1, member offset 24
        ];
        for (logical_chunk, want_disk, want_off) in expected {
            let cursor = logical_chunk * chunk;
            let step = raid5_step(cursor, chunk, n, Raid5Layout::LeftSymmetric, chunk as usize);
            assert_eq!(
                step.member_index, want_disk,
                "logical chunk {logical_chunk}"
            );
            assert_eq!(
                step.member_offset, want_off,
                "logical chunk {logical_chunk}"
            );
            assert_eq!(step.take_len, chunk as usize);
        }
    }

    /// 3-disk RAID5 left-asymmetric: same parity rotation as left-symmetric
    /// but data fills slots in disk-index order, skipping parity.
    ///   stripe 0: parity=2, data → disks 0, 1 (same as symmetric for stripe 0
    ///              because parity is the highest slot)
    ///   stripe 1: parity=1, data → disks 0, 2 (asymmetric: skip parity)
    ///   stripe 2: parity=0, data → disks 1, 2
    #[test]
    fn raid5_left_asymmetric_three_disk_layout() {
        let chunk = 8u64;
        let n = 3u64;
        let cases = [
            (0u64, 0usize),
            (1u64, 1usize),
            (2u64, 0usize),
            (3u64, 2usize),
            (4u64, 1usize),
            (5u64, 2usize),
        ];
        for (logical_chunk, want_disk) in cases {
            let cursor = logical_chunk * chunk;
            let step = raid5_step(
                cursor,
                chunk,
                n,
                Raid5Layout::LeftAsymmetric,
                chunk as usize,
            );
            assert_eq!(step.member_index, want_disk, "chunk {logical_chunk}");
        }
    }

    /// Synthetic byte-buffer member used by the integration smoke tests.
    /// Each member carries a fixed 1-byte fill so reading a stripe back
    /// reveals which physical member contributed each chunk.
    struct FillMember(u8, u64);
    impl SectorSource for FillMember {
        fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
            if offset >= self.1 {
                return Ok(Vec::new());
            }
            let take = (self.1 - offset).min(len as u64) as usize;
            Ok(vec![self.0; take])
        }
        fn size(&self) -> u64 {
            self.1
        }
    }

    fn fill_member(byte: u8, size: u64) -> StripedMember {
        StripedMember {
            reader: Arc::new(FillMember(byte, size)),
            data_offset_bytes: 0,
            data_size_bytes: size,
        }
    }

    /// `RaidVolume::set` constructs a RAID0 reader. A 4-byte read crossing
    /// the chunk boundary at offset 4 must yield 2 bytes from member 0
    /// followed by 2 bytes from member 1 — verifying the dispatch path
    /// from the unified `read_at` through `stripe_step`.
    #[test]
    fn raid_volume_set_dispatches_raid0_reads_through_stripe_step() {
        let members = vec![fill_member(0xAA, 32), fill_member(0xBB, 32)];
        let vol = RaidVolume::set(RaidLevel::Raid0, 512, 4, None, members);
        assert_eq!(vol.total_bytes(), 64);
        let bytes = vol.read_at(2, 4).expect("read");
        assert_eq!(bytes, vec![0xAA, 0xAA, 0xBB, 0xBB]);
    }

    /// `RaidVolume::set` with `RaidLevel::Raid5` dispatches to `raid5_step`
    /// — first two logical chunks of a 3-disk left-symmetric array land on
    /// physical disks 0 and 1 (parity=2 for stripe row 0).
    #[test]
    fn raid_volume_set_dispatches_raid5_reads_through_raid5_step() {
        let members = vec![
            fill_member(0xAA, 64),
            fill_member(0xBB, 64),
            fill_member(0xCC, 64),
        ];
        let vol = RaidVolume::set(
            RaidLevel::Raid5(Raid5Layout::LeftSymmetric),
            512,
            8,
            None,
            members,
        );
        // Logical capacity = (n - 1) × per-member = 2 × 64 = 128.
        assert_eq!(vol.total_bytes(), 128);
        let bytes = vol.read_at(0, 16).expect("read");
        // Bytes 0..7 → disk 0 (0xAA), bytes 8..15 → disk 1 (0xBB).
        assert_eq!(&bytes[..8], &[0xAA; 8]);
        assert_eq!(&bytes[8..], &[0xBB; 8]);
    }

    /// `logical_size_bytes = Some(n)` clamps reads to the SB-recorded
    /// size even when the member sum would imply a larger volume.
    #[test]
    fn raid_volume_honours_explicit_logical_size() {
        let members = vec![fill_member(0xAA, 64), fill_member(0xBB, 64)];
        let vol = RaidVolume::set(RaidLevel::Raid0, 512, 8, Some(16), members);
        assert_eq!(vol.total_bytes(), 16);
        // A read past the explicit cap returns only what fits.
        let bytes = vol.read_at(0, 1024).expect("read");
        assert_eq!(bytes.len(), 16);
    }

    /// `RaidLevel::from_raw` accepts kernel-recognised inputs and rejects
    /// unsupported levels / out-of-range RAID5 layouts.
    #[test]
    fn raid_level_from_raw_filters_unsupported_combinations() {
        assert_eq!(RaidLevel::from_raw(0, 0), Some(RaidLevel::Raid0));
        assert_eq!(
            RaidLevel::from_raw(5, 2),
            Some(RaidLevel::Raid5(Raid5Layout::LeftSymmetric))
        );
        assert_eq!(RaidLevel::from_raw(5, 99), None);
        assert_eq!(RaidLevel::from_raw(1, 0), None); // RAID1 not supported
        assert_eq!(RaidLevel::from_raw(6, 2), None); // RAID6 not supported
    }

    /// Byte-buffer member used to test parity reconstruction (unlike
    /// `FillMember`, content varies per offset so XOR is meaningful).
    struct VecMember(Vec<u8>);
    impl SectorSource for VecMember {
        fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
            let start = (offset as usize).min(self.0.len());
            let end = (start + len).min(self.0.len());
            Ok(self.0[start..end].to_vec())
        }
        fn size(&self) -> u64 {
            self.0.len() as u64
        }
    }

    fn vec_member(bytes: Vec<u8>) -> StripedMember {
        let size = bytes.len() as u64;
        StripedMember {
            reader: Arc::new(VecMember(bytes)),
            data_offset_bytes: 0,
            data_size_bytes: size,
        }
    }

    /// Degraded RAID5 read: a 3-member array where `m2 = m0 ^ m1` makes
    /// `XOR(all members) == 0` at every offset. Reconstructing ANY single
    /// missing slot must reproduce the exact same striped logical bytes as
    /// the non-degraded read — confirming the XOR rebuild is correct for
    /// each rotating column regardless of which one is dropped.
    #[test]
    fn raid5_degraded_read_reconstructs_missing_column() {
        let size = 64usize;
        let m0: Vec<u8> = (0..size)
            .map(|i| (i as u8).wrapping_mul(7).wrapping_add(1))
            .collect();
        let m1: Vec<u8> = (0..size)
            .map(|i| (i as u8).wrapping_mul(13).wrapping_add(5))
            .collect();
        let m2: Vec<u8> = m0.iter().zip(&m1).map(|(a, b)| a ^ b).collect();

        let members = vec![
            vec_member(m0.clone()),
            vec_member(m1.clone()),
            vec_member(m2.clone()),
        ];
        let full = RaidVolume::set(
            RaidLevel::Raid5(Raid5Layout::LeftSymmetric),
            512,
            8,
            None,
            members,
        );
        let expected = full.read_at(0, 128).expect("full read");
        assert_eq!(expected.len(), 128); // (3 - 1) * 64

        for missing in 0..3usize {
            let members = vec![
                vec_member(m0.clone()),
                vec_member(m1.clone()),
                vec_member(m2.clone()),
            ];
            let degraded = RaidVolume::set_degraded(
                RaidLevel::Raid5(Raid5Layout::LeftSymmetric),
                512,
                8,
                None,
                members,
                Some(missing),
            );
            let got = degraded.read_at(0, 128).expect("degraded read");
            assert_eq!(
                got, expected,
                "reconstruction mismatch for missing slot {missing}"
            );
        }
    }
}
