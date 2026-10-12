//! Filesystem probes on assembled RAID sources.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct AssembledFilesystem {
    pub(super) family: AssembledFsFamily,
    pub(super) fsid_hex: Option<String>,
    pub(super) bytes_used: Option<u64>,
    pub(super) total_bytes: Option<u64>,
    pub(super) num_devices: Option<u64>,
    pub(super) sector_size: Option<u32>,
    pub(super) node_size: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum AssembledFsFamily {
    Btrfs,
    Ext4,
}

impl AssembledFilesystem {
    pub(super) fn kind_str(&self) -> &'static str {
        match self.family {
            AssembledFsFamily::Btrfs => "btrfs",
            AssembledFsFamily::Ext4 => "ext4",
        }
    }

    pub(super) fn kind_enum(&self) -> PartitionKind {
        match self.family {
            AssembledFsFamily::Btrfs => PartitionKind::Btrfs,
            AssembledFsFamily::Ext4 => PartitionKind::Ext4,
        }
    }
}

pub(super) fn probe_assembled_filesystem(
    raid: &(dyn SectorSource + Send + Sync),
) -> anyhow::Result<Option<AssembledFilesystem>> {
    if let Some(fs) = probe_btrfs(raid)? {
        return Ok(Some(fs));
    }
    if let Some(fs) = probe_ext4(raid)? {
        return Ok(Some(fs));
    }
    Ok(None)
}

pub(super) fn probe_btrfs(
    raid: &(dyn SectorSource + Send + Sync),
) -> anyhow::Result<Option<AssembledFilesystem>> {
    if raid.size() < BTRFS_PROBE_END_OFFSET + 0x300 {
        return Ok(None);
    }
    let buf = raid
        .read(BTRFS_PRIMARY_SUPERBLOCK, 0x1000)
        .context("reading btrfs superblock from assembled array")?;
    if buf.len() < 0x100 || &buf[0x40..0x48] != BTRFS_MAGIC {
        return Ok(None);
    }
    // Field layout: offset relative to superblock start (0x10000 logical /
    // 0 in `buf`). See `struct btrfs_super_block` in
    // include/uapi/linux/btrfs_tree.h:
    //   csum 0..0x20, fsid 0x20..0x30, bytenr 0x30..0x38, flags 0x38..0x40,
    //   magic 0x40..0x48, generation 0x48..0x50, root 0x50..0x58,
    //   chunk_root 0x58..0x60, log_root 0x60..0x68,
    //   log_root_transid 0x68..0x70, total_bytes 0x70..0x78,
    //   bytes_used 0x78..0x80, root_dir_objectid 0x80..0x88,
    //   num_devices 0x88..0x90, sectorsize 0x90..0x94, nodesize 0x94..0x98.
    let fsid_bytes = &buf[0x20..0x30];
    let fsid_hex = fsid_bytes.iter().map(|b| format!("{b:02x}")).collect();
    let total_bytes = u64::from_le_bytes(buf[0x70..0x78].try_into().unwrap_or([0; 8]));
    let bytes_used = u64::from_le_bytes(buf[0x78..0x80].try_into().unwrap_or([0; 8]));
    let num_devices = u64::from_le_bytes(buf[0x88..0x90].try_into().unwrap_or([0; 8]));
    let sector_size = u32::from_le_bytes(buf[0x90..0x94].try_into().unwrap_or([0; 4]));
    let node_size = u32::from_le_bytes(buf[0x94..0x98].try_into().unwrap_or([0; 4]));
    Ok(Some(AssembledFilesystem {
        family: AssembledFsFamily::Btrfs,
        fsid_hex: Some(fsid_hex),
        bytes_used: Some(bytes_used),
        total_bytes: Some(total_bytes),
        num_devices: Some(num_devices),
        sector_size: Some(sector_size),
        node_size: Some(node_size),
    }))
}

pub(super) fn probe_ext4(
    raid: &(dyn SectorSource + Send + Sync),
) -> anyhow::Result<Option<AssembledFilesystem>> {
    // Ext{2,3,4} primary superblock at offset 0x400; magic `0xEF53` lives
    // at superblock offset 0x38 (= absolute 0x438).
    if raid.size() < 0x500 {
        return Ok(None);
    }
    let buf = raid
        .read(0x400, 0x100)
        .context("reading ext4 superblock from assembled array")?;
    if buf.len() < 0x80 || buf[0x38] != 0x53 || buf[0x39] != 0xEF {
        return Ok(None);
    }
    // Layout per `struct ext2_super_block` (fs/ext4/ext4.h, all LE):
    //   s_inodes_count 0x00, s_blocks_count_lo 0x04, s_log_block_size 0x18,
    //   s_magic 0x38, s_uuid 0x68..0x78, s_volume_name 0x78..0x88.
    let blocks_count_lo = u32::from_le_bytes(buf[0x04..0x08].try_into().unwrap_or([0; 4])) as u64;
    let log_block_size = u32::from_le_bytes(buf[0x18..0x1C].try_into().unwrap_or([0; 4]));
    let block_size = 1024u64 << log_block_size.min(20); // clamp absurd values
    let total_bytes = blocks_count_lo.saturating_mul(block_size);
    let fsid_hex: String = buf[0x68..0x78].iter().map(|b| format!("{b:02x}")).collect();
    Ok(Some(AssembledFilesystem {
        family: AssembledFsFamily::Ext4,
        fsid_hex: Some(fsid_hex),
        bytes_used: None,
        total_bytes: Some(total_bytes),
        num_devices: None,
        sector_size: Some(block_size as u32),
        node_size: None,
    }))
}
