//! End-to-end RAID auto-detection across multiple loaded disk images.
//!
//! The pipeline runs in three passes:
//!
//! 1. **Candidate enumeration.** Walk every loaded image's MBR and LVM
//!    text metadata, materialising every potential RAID member as an
//!    `Arc<dyn SectorSource>`. LV-on-LVM members use `LinearLvSource`
//!    to concatenate PVs across images; whole-disk / whole-partition
//!    members expose devices tagged `Mdadm`.
//!    The two shapes converge into a single `CandidateMember` list so
//!    Pass 2 has one code path instead of two.
//! 2. **Superblock probe.** Each candidate is fed through the
//!    `mdadm_super::probe` registry — a static, priority-ordered table
//!    of `MdadmSuperblockProbe` impls (currently v1.2 sector-8 and v1.1
//!    sector-0). The first match wins and the candidate is grouped
//!    into `members_by_set` keyed by the SB's `set_uuid`.
//! 3. **Assembly.** Each set's members are slot-ordered via the
//!    superblock's `dev_roles[dev_number]` table (so a spare-replaces
//!    event that bumps `dev_number` past `raid_disks` still maps to the
//!    right slot), and materialised as a `Raid0Source` or
//!    `Raid5Source` virtual device. The assembled device is then
//!    probed for ext4 and btrfs.
//!
//! The output is purely descriptive — every layer is reported via
//! `RaidStack` so the frontend can render the same picture that UFS
//! Explorer's left rail shows. No persistent state is mutated; the IPC
//! layer can re-run detection on demand.

pub mod heuristic;
pub mod lvm_meta;
pub mod mdadm_super;
pub mod source;
pub mod xfs_probe;

mod manual;
pub use manual::manual_assemble;
mod detect;
pub use detect::detect_with_sources;
mod members;
use members::*;
mod probe;
use probe::*;

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::core::evidence::image::DiskImage;
use crate::core::evidence::partitions::{
    detect_partitions_with_meta, refine_partition_kinds, Partition, PartitionKind, SectorSource,
    BTRFS_MAGIC, BTRFS_PRIMARY_SUPERBLOCK, BTRFS_PROBE_END_OFFSET,
};
use lvm_meta::LvmPvMetadata;
use mdadm_super::MdadmSuperblock;
use source::{
    LinearLvSource, LvByteSegment, MemberReader, RaidLevel, RaidVolume, StripedMember,
    WholeDiskMember,
};

/// Top-level result. Every section is filled in best-effort: if PV parsing
/// fails on one image, we still report whatever else worked. Empty
/// sub-vectors mean "nothing detected" rather than "error".
#[derive(Debug, Clone, Serialize)]
pub struct RaidStack {
    pub images: Vec<RaidImage>,
    pub volume_groups: Vec<RaidVolumeGroup>,
    pub arrays: Vec<RaidArray>,
    pub filesystems: Vec<RaidFilesystem>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidImage {
    pub image_id: String,
    pub path: String,
    pub size_bytes: u64,
    pub partition_count: usize,
    pub lvm_pv_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidVolumeGroup {
    pub vg_uuid: String,
    pub vg_name: String,
    pub extent_size_sectors: u64,
    pub members: Vec<RaidVgMember>,
    pub logical_volumes: Vec<RaidLogicalVolume>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidVgMember {
    pub image_id: String,
    pub partition_id: u8,
    pub pv_local_id: String,
    pub pv_uuid: String,
    pub pe_start_sectors: u64,
    pub partition_byte_offset: u64,
    pub partition_byte_length: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidLogicalVolume {
    pub name: String,
    pub size_bytes: u64,
    pub segments: Vec<RaidLvSegment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidLvSegment {
    pub pv_local_id: String,
    pub image_id: String,
    pub partition_id: u8,
    pub byte_offset: u64,
    pub byte_length: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidArray {
    pub set_uuid: String,
    pub set_name: String,
    /// `0` for RAID0, `1` for RAID1, `5` for RAID5, etc. RAID0 and RAID5
    /// are currently materialised; other levels surface as descriptive
    /// records with `assembled_image_id == None`.
    pub level: i32,
    /// Raw `layout` field from the v1.2 SB — only meaningful for RAID5/6
    /// (0..3 select left/right × symmetric/asymmetric). Reported so the
    /// frontend can disclose it, but assembly currently relies on the
    /// kernel default (`2` LEFT_SYMMETRIC) for any value the parser
    /// recognises.
    pub layout: u32,
    pub chunksize_sectors: u32,
    pub raid_disks: u32,
    pub data_offset_sectors: u64,
    /// Total assembled size of the array data area in bytes.
    pub assembled_bytes: u64,
    pub members: Vec<RaidArrayMember>,
    /// `Some(image_id)` after the IPC layer has materialised this array as a
    /// synthetic `DiskImage`. `None` for unsupported levels (mirrors etc.)
    /// or when assembly succeeded but no recognised filesystem was found.
    pub assembled_image_id: Option<String>,
    /// `true` when this array was reconstructed by the metadata-less heuristic
    /// (`raid::heuristic`) — its parameters were *recovered* by brute-force +
    /// filesystem validation, not read from an mdadm superblock. The frontend
    /// surfaces this so an examiner knows the layout was inferred.
    #[serde(default)]
    pub heuristic: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidArrayMember {
    pub dev_number: u32,
    /// Slot this member occupies within the array (0..raid_disks-1),
    /// derived from `dev_roles[dev_number]`. `None` only when the
    /// superblock's role table marks the entry as spare/faulty or the
    /// recorded slot exceeds `raid_disks`; such members are skipped
    /// during assembly.
    pub slot: Option<u32>,
    pub device_uuid: String,
    pub data_size_sectors: u64,
    /// `true` when this member's payload was rebuilt from parity rather than
    /// read from the disk — i.e. a dropped/stale RAID5 column reconstructed by
    /// the metadata-less heuristic. Always `false` for mdadm-assembled members.
    #[serde(default)]
    pub reconstructed: bool,
    /// Origin describing where the member's payload comes from. Either an
    /// LV-on-LVM identifier (`检材3` shape) or a whole-disk byte range
    /// (`服务器部分/server*.001` shape).
    pub origin: RaidMemberOrigin,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum RaidMemberOrigin {
    Lv {
        vg_uuid: String,
        vg_name: String,
        lv_name: String,
        size_bytes: u64,
    },
    /// Whole-image (or whole-partition) member: the SB sits at byte
    /// `0x1000` of a `PartitionKind::Mdadm` partition slice rather than
    /// inside an LV. `byte_offset` / `byte_length` are absolute against
    /// the source `DiskImage`.
    Whole {
        image_id: String,
        partition_id: u8,
        byte_offset: u64,
        byte_length: u64,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct RaidFilesystem {
    pub set_uuid: String,
    pub kind: &'static str,
    pub fsid: Option<String>,
    pub total_bytes: Option<u64>,
    pub bytes_used: Option<u64>,
    pub num_devices: Option<u64>,
    pub sector_size: Option<u32>,
    pub node_size: Option<u32>,
}

/// Concrete materialisation of one RAID0 / RAID5 array — the assembled
/// virtual block device plus the metadata the IPC layer needs to register
/// it as a synthetic `DiskImage`. Emitted by `detect_with_sources` for
/// every array whose filesystem probe succeeds; not emitted for
/// unassembled mirror/parity arrays where the layout cannot be replayed
/// (RAID1 chooses any member; RAID6 needs Q-recovery for missing data
/// disks).
#[derive(Clone)]
pub struct AssembledArray {
    pub set_uuid: String,
    pub set_name: String,
    /// Wrapped in `Arc` so the `ImageSource::Assembled` variant can take an
    /// owned `Arc<dyn SectorSource + Send + Sync>` and the caller can keep
    /// other handles for diagnostics or testing.
    pub source: Arc<dyn SectorSource + Send + Sync>,
    pub size_bytes: u64,
    /// Filesystem detected on the assembled device — currently `Btrfs` or
    /// `Ext4`. Future probes (XFS, NTFS, …) drop in without changing this
    /// signature.
    pub fs_kind: PartitionKind,
    /// Partition table detected on the assembled device, when the array is a
    /// partitioned disk (e.g. a hardware-RAID member set carrying an MBR/GPT
    /// rather than a bare filesystem). `Some(parts)` makes the IPC layer
    /// register the synthetic image with this table instead of a single
    /// whole-disk `fs_kind` partition; `None` preserves the bare-filesystem
    /// behaviour used by the mdadm/LVM path.
    pub partitions: Option<Vec<Partition>>,
}

/// Format the `Partition.label` shown on a synthetic assembled-image
/// partition: `"<fs> (md/<set-name-or-uuid>)"`. Lives here so the
/// `kind → string` map stays alongside `AssembledArray` rather than in
/// the IPC layer.
pub fn synthetic_partition_label(array: &AssembledArray) -> String {
    let identifier = if array.set_name.is_empty() {
        array.set_uuid.as_str()
    } else {
        array.set_name.as_str()
    };
    format!("{} (md/{})", kind_label(array.fs_kind), identifier)
}

/// Lowercase label for a partition/filesystem kind, shared by the synthetic
/// label builder and the manual-assemble IPC preview.
pub fn kind_label(kind: PartitionKind) -> &'static str {
    match kind {
        PartitionKind::Btrfs => "btrfs",
        PartitionKind::Ext4 => "ext4",
        PartitionKind::Ntfs => "ntfs",
        PartitionKind::Refs => "refs",
        PartitionKind::Fat32 => "fat32",
        PartitionKind::Fat12 => "fat12",
        PartitionKind::Fat16 => "fat16",
        PartitionKind::ExFat => "exfat",
        PartitionKind::Cdfs => "cdfs",
        PartitionKind::Squashfs => "squashfs",
        PartitionKind::Lvm => "lvm",
        PartitionKind::Bitlocker => "bitlocker",
        PartitionKind::BiosBoot => "biosboot",
        PartitionKind::ApfsContainer => "apfs",
        PartitionKind::HfsPlus => "hfsplus",
        PartitionKind::HfsX => "hfsx",
        PartitionKind::Luks => "luks",
        PartitionKind::Filevault => "filevault",
        PartitionKind::Apm => "apm",
        PartitionKind::Bsd => "bsd",
        PartitionKind::Swap => "swap",
        PartitionKind::Extended => "extended",
        PartitionKind::Mdadm => "mdadm",
        PartitionKind::Ufs => "ufs",
        PartitionKind::Xfs => "xfs",
        PartitionKind::Zfs => "zfs",
        PartitionKind::Unknown => "unknown",
    }
}

/// Scan every loaded image and assemble whatever LVM-on-disk → mdadm →
/// filesystem stack is hiding inside.
pub fn detect(images: &[Arc<DiskImage>]) -> RaidStack {
    let (stack, _assembled) = detect_with_sources(images);
    stack
}

/// Parameters for a user-driven manual RAID reconstruction (the "RAID builder"
/// fallback for arrays the automatic heuristic cannot crack). Members are given
/// in column/slot order; the geometry is taken verbatim, not searched.
#[derive(Debug, Clone, Deserialize)]
pub struct ManualRaidParams {
    /// Member image ids (already loaded), in column (slot) order.
    pub member_image_ids: Vec<String>,
    /// RAID level: `0` (striped) or `5` (single parity).
    pub level: i32,
    /// RAID5 layout 0..3 (left/right × asymmetric/symmetric); ignored for RAID0.
    pub layout: u32,
    /// Stripe/chunk size in 512-byte sectors.
    pub chunk_sectors: u32,
    /// Per-member reserved-header size in 512-byte sectors (data offset).
    pub data_offset_sectors: u64,
    /// Column (slot) index of a dropped member to rebuild from parity, if any
    /// (RAID5 only). `None` assembles with every member read directly.
    pub missing_slot: Option<usize>,
}
