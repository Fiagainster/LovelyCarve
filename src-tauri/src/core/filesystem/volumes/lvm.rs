//! Read-only LVM2 physical-volume parser.
//!
//! Parses the textual metadata block embedded in the first ~4 MiB of an LVM2
//! PV, then materializes each logical volume as a slice on the underlying
//! `DiskImage`. Only enough of the metadata is decoded to enumerate logical
//! volumes and resolve their first FLAT segment back to a byte offset.
//!
//! Active-snapshot selection follows the on-disk binary `mda_header`: the
//! `LABELONE` PV label points at the `PhysicalVolumeHeader`, whose first
//! `metadata_descriptor` points at a `MetadataAreaHeader`, whose first
//! `location_descriptor` carries the byte offset and size of the currently
//! active metadata text. That's the same pointer LVM itself uses, so we
//! don't have to guess which copy of the rotating snapshot buffer is
//! current. When the binary layout doesn't match (truncated probe, custom
//! MDA layout, on-disk wrap), we fall back to scanning the whole probe and
//! picking the VG block with the highest `seqno`.

use crate::core::evidence::image::{detect_filesystem, DiskImage, Partition, PartitionKind};
use crate::core::evidence::partitions::lvm2::metadata::{
    active_logical_volumes_block, locate_active_metadata_region, lvm_metadata_text, named_blocks,
    parse_assignment_u64, parse_first_segment,
};
use crate::core::filesystem::tree::fs_tree::{FsNode, Mactb, NodeKind};
use crate::core::filesystem::unalloc;

#[derive(Debug)]
pub struct LvmVolume {
    partition_id: u8,
    /// Absolute byte offset of the PV data area start (`pe_start` from the LVM2
    /// metadata, in image bytes). Below this lies the LVM label + metadata,
    /// not allocatable PE space.
    pe_start_bytes: u64,
    /// Absolute byte offset of the PV's end (partition end), bounding the data
    /// area. `[pe_start_bytes, pv_end_bytes)` is the allocatable region.
    pv_end_bytes: u64,
    pub logical_volumes: Vec<LvmLogicalVolume>,
}

#[derive(Debug, Clone)]
pub struct LvmLogicalVolume {
    pub index: usize,
    pub name: String,
    pub absolute_offset: u64,
    pub size_bytes: u64,
    pub kind: PartitionKind,
}

impl LvmVolume {
    pub fn open(image: &DiskImage, partition: &Partition) -> anyhow::Result<Self> {
        if partition.kind != PartitionKind::Lvm {
            anyhow::bail!("partition {} is not an LVM2 physical volume", partition.id);
        }

        let partition_offset = partition.start_lba * image.sector_size as u64;
        let probe_len =
            (partition.length_lba * image.sector_size as u64).min(4 * 1024 * 1024) as usize;
        let probe = image.read_range(partition_offset, probe_len)?;
        // Binary mda_header points at the active snapshot directly; fall
        // through to a full-buffer text scan only when the header doesn't
        // parse (corrupt, custom layout, or the active region wrapped the
        // rotating buffer).
        let metadata = locate_active_metadata_region(&probe)
            .and_then(|(start, end)| lvm_metadata_text(&probe[start..end]))
            .or_else(|| lvm_metadata_text(&probe))
            .ok_or_else(|| anyhow::anyhow!("LVM2 metadata not found"))?;
        let extent_size = parse_assignment_u64(&metadata, "extent_size")
            .ok_or_else(|| anyhow::anyhow!("LVM extent_size not found"))?;
        let pe_start = parse_assignment_u64(&metadata, "pe_start")
            .ok_or_else(|| anyhow::anyhow!("LVM pe_start not found"))?;
        let logical_block = active_logical_volumes_block(&metadata)
            .ok_or_else(|| anyhow::anyhow!("LVM logical_volumes block not found"))?;

        let mut logical_volumes = Vec::new();
        for (name, lv_block) in named_blocks(logical_block) {
            let Some((physical_extent, extent_count)) = parse_first_segment(&lv_block) else {
                continue;
            };
            // Extent math is attacker-influenced (the metadata is parsed from
            // the image), so every multiply/add is checked: a crafted or
            // corrupt extent_size/count must skip the LV with a diagnostic
            // rather than panic in debug or wrap to a bogus offset in release.
            let sector_bytes = image.sector_size as u64;
            let geometry = physical_extent
                .checked_mul(extent_size)
                .and_then(|pe| pe.checked_add(pe_start))
                .and_then(|start_sector| {
                    let sectors = extent_count.checked_mul(extent_size)?;
                    let absolute_offset = start_sector
                        .checked_mul(sector_bytes)
                        .and_then(|off| partition_offset.checked_add(off))?;
                    let size_bytes = sectors.checked_mul(sector_bytes)?;
                    Some((absolute_offset, size_bytes))
                });
            let Some((absolute_offset, size_bytes)) = geometry else {
                tracing::warn!(
                    "LVM LV {name}: extent geometry overflows u64 \
                     (pe={physical_extent}, count={extent_count}, extent_size={extent_size}); skipping"
                );
                continue;
            };
            let signature = image.read_range(absolute_offset, 4096)?;
            let kind = detect_filesystem(&signature, PartitionKind::Unknown);
            logical_volumes.push(LvmLogicalVolume {
                index: logical_volumes.len(),
                name,
                absolute_offset,
                size_bytes,
                kind,
            });
        }

        Ok(Self {
            partition_id: partition.id,
            pe_start_bytes: partition_offset.saturating_add(pe_start * image.sector_size as u64),
            pv_end_bytes: partition_offset
                .saturating_add(partition.length_lba * image.sector_size as u64),
            logical_volumes,
        })
    }

    pub fn logical_volume_nodes(&self) -> anyhow::Result<Vec<FsNode>> {
        Ok(self
            .logical_volumes
            .iter()
            .map(|lv| FsNode {
                id: format!("lvm:{}:{}", self.partition_id, lv.index),
                // 导航名就用 LV 名本身（VG 内唯一）："LV xxx · Ext4" 的前缀
                // 和文件系统后缀只会让面包屑/全路径变长，类型看详情面板。
                name: lv.name.clone(),
                parent_id: Some(format!("part:{}", self.partition_id)),
                kind: if is_navigable_lv_kind(lv.kind) {
                    NodeKind::Dir
                } else {
                    NodeKind::Unallocated
                },
                size: lv.size_bytes,
                deleted: false,
                inode: None,
                timestamps: Mactb::default(),
                link_target: None,
                has_snapshot: false,
            })
            .collect())
    }

    /// Virtual "未使用空间N" files for the PE space in this volume group that
    /// no logical volume claims. The allocatable region is
    /// `[pe_start_bytes, pv_end_bytes)`; subtracting every LV's
    /// `[absolute_offset, +size_bytes)` yields the free segments. One LVM2
    /// extent (typically 4 MiB) is the noise floor — smaller leftovers are
    /// alignment slack, not meaningful free space. Single-PV only; spanning a
    /// VG across multiple PVs is out of scope for this parser.
    pub fn vg_unalloc_nodes(&self) -> Vec<FsNode> {
        if self.pv_end_bytes <= self.pe_start_bytes {
            return Vec::new();
        }
        let occupied: Vec<(u64, u64)> = self
            .logical_volumes
            .iter()
            .map(|lv| (lv.absolute_offset, lv.size_bytes))
            .collect();
        let total = self.pv_end_bytes - self.pe_start_bytes;
        unalloc::free_intervals(self.pe_start_bytes, total, &occupied, 4 * 1024 * 1024)
            .into_iter()
            .enumerate()
            .map(|(idx, (offset, len))| FsNode {
                id: format!("unalloc:lvm:{}:0x{offset:x}:0x{len:x}", self.partition_id),
                name: format!("未使用空间{}", idx + 1),
                parent_id: Some(format!("part:{}", self.partition_id)),
                kind: NodeKind::File,
                size: len,
                deleted: false,
                inode: None,
                timestamps: Mactb::default(),
                link_target: None,
                has_snapshot: false,
            })
            .collect()
    }
}

/// Filesystems we can navigate into when they appear as the contents of an
/// LVM2 logical volume. Mirrors the per-FS `Lvm` source variants in
/// `core::filesystem::volumes::{ext4, xfs, btrfs, ntfs}`. Add a new entry here whenever
/// another FS adapter grows an `Lvm` source.
fn is_navigable_lv_kind(kind: PartitionKind) -> bool {
    matches!(
        kind,
        PartitionKind::Ext4 | PartitionKind::Xfs | PartitionKind::Btrfs | PartitionKind::Ntfs
    )
}

pub fn parse_lvm_id(id: &str) -> Option<(&str, usize)> {
    let mut parts = id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("lvm"), Some(part_id), Some(lv_index), None) => {
            lv_index.parse().ok().map(|idx| (part_id, idx))
        }
        _ => None,
    }
}
