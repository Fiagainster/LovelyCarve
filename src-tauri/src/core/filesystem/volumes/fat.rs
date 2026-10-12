//! Read-only FAT (FAT12 / FAT16 / FAT32) volume adapter built on the
//! vendored `fatfs` crate.
//!
//! Each backend operation instantiates a fresh `fatfs::FileSystem` over a
//! transient cursor against the partition slice (mirrors `core::ntfs_volume`)
//! — there is no shared mutable state, so the dispatcher can fan out
//! concurrent listings / reads without per-volume locking.
//!
//! Node IDs encode the absolute UTF-8 path inside the volume as hex bytes:
//! `fat:part:{N}:{hex(/path/to/entry)}`. The empty-hex form denotes the
//! partition root and is never emitted directly — the partition node itself
//! stands in for the root.

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, TimeZone, Utc};
use fatfs::{Date as FatDate, DateTime as FatDateTime, FsOptions, StdIoWrapper};

use crate::core::evidence::image::find_partition;
use crate::core::evidence::image::{DiskImage, Partition};
use crate::core::evidence::partitions::fat::recover_fat32_boot_sector;
use crate::core::evidence::partitions::PartitionSlice;
use crate::core::filesystem::tree::fs_tree::{
    detect_mime_bytes, shannon_entropy, FsDataRun, FsNode, FsStat, Mactb, NodeKind,
};

const SAMPLE_BYTES: usize = 4096;
const STREAM_FLOOR: usize = 64 * 1024;
const BOOT_SECTOR_LEN: usize = 512;
const BOOT_SIGNATURE: [u8; 2] = [0x55, 0xAA];
const FAT32_BACKUP_BOOT_SECTOR: u64 = 6;
const FAT32_LABEL_OFFSET: usize = 82;
const FAT32_LABEL_LEN: usize = 8;

#[derive(Debug, Clone, Copy)]
pub enum FatSource {
    Partition(u8),
}

/// Parse a `fat:part:{N}:{hex(path)}` ID. Returns the source plus the
/// decoded UTF-8 path (with a leading `/`, or empty for the volume root).
pub fn parse_fat_id(id: &str) -> Option<(FatSource, String)> {
    let mut parts = id.splitn(4, ':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("fat"), Some("part"), Some(part_id), Some(hex_path)) => {
            let part_id = part_id.parse().ok()?;
            let path = if hex_path.is_empty() {
                String::new()
            } else {
                let bytes = hex::decode(hex_path).ok()?;
                String::from_utf8(bytes).ok()?
            };
            Some((FatSource::Partition(part_id), path))
        }
        _ => None,
    }
}

pub fn open_fat_source(image: &Arc<DiskImage>, source: FatSource) -> Result<FatVolume> {
    match source {
        FatSource::Partition(part_id) => {
            let partition = find_partition(image, &part_id.to_string())?;
            FatVolume::open(Arc::clone(image), partition)
        }
    }
}

pub struct FatVolume {
    image: Arc<DiskImage>,
    partition_id: u8,
    base_offset: u64,
    length_bytes: u64,
    boot_sector_override: Option<[u8; BOOT_SECTOR_LEN]>,
}

impl FatVolume {
    pub fn open(image: Arc<DiskImage>, partition: &Partition) -> Result<Self> {
        let sector_size = image.sector_size as u64;
        let base_offset = partition.start_lba * sector_size;
        let length_bytes = partition.length_lba * sector_size;
        let boot_sector_override =
            find_fat32_recovered_boot_sector(&image, base_offset, length_bytes, sector_size)?;
        let volume = Self {
            image,
            partition_id: partition.id,
            base_offset,
            length_bytes,
            boot_sector_override,
        };
        // Eager-validate the boot sector so the caller sees parse errors at
        // open time instead of only when listing the root.
        volume.with_fs(|_| Ok(()))?;
        Ok(volume)
    }

    /// Lists the children of a path inside the volume. The path uses Unix-
    /// style separators with a leading `/` (or the empty string for root).
    pub fn children(&self, path: &str) -> Result<Vec<FsNode>> {
        let parent_id = self.node_id(path);
        let parent_path = path.to_string();
        self.with_fs(|fs| {
            let dir = if path.is_empty() || path == "/" {
                fs.root_dir()
            } else {
                let rel = path.trim_start_matches('/');
                fs.root_dir()
                    .open_dir(rel)
                    .map_err(|err| anyhow!("FAT open_dir {path}: {err}"))?
            };

            let mut out = Vec::new();
            for entry in dir.iter() {
                let entry = entry.map_err(|err| anyhow!("FAT dir entry under {path}: {err}"))?;
                let name = entry.file_name();
                if name == "." || name == ".." || name.is_empty() {
                    continue;
                }
                if entry
                    .attributes()
                    .contains(fatfs::FileAttributes::VOLUME_ID)
                {
                    continue;
                }
                let child_path = join_path(&parent_path, &name);
                let kind = if entry.is_dir() {
                    NodeKind::Dir
                } else {
                    NodeKind::File
                };
                let size = if entry.is_dir() { 0 } else { entry.len() };
                out.push(FsNode {
                    id: self.node_id(&child_path),
                    name,
                    parent_id: Some(parent_id.clone()),
                    kind,
                    size,
                    deleted: false,
                    inode: None,
                    timestamps: entry_timestamps(&entry),
                    link_target: None,
                    has_snapshot: false,
                });
            }
            out.sort_by(|a, b| {
                let ak = u8::from(a.kind != NodeKind::Dir);
                let bk = u8::from(b.kind != NodeKind::Dir);
                ak.cmp(&bk)
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
            Ok(out)
        })
    }

    pub fn read_file(&self, path: &str, max_bytes: usize) -> Result<Vec<u8>> {
        if path.is_empty() {
            anyhow::bail!("cannot read FAT volume root as a file");
        }
        let rel = path.trim_start_matches('/').to_string();
        self.with_fs(move |fs| {
            let mut file = fs
                .root_dir()
                .open_file(&rel)
                .map_err(|err| anyhow!("FAT open_file /{rel}: {err}"))?;
            let mut out = Vec::new();
            let mut buf = vec![0u8; STREAM_FLOOR];
            while out.len() < max_bytes {
                let want = (max_bytes - out.len()).min(buf.len());
                let read = std::io::Read::read(&mut file, &mut buf[..want])
                    .map_err(|err| anyhow!("FAT read /{rel}: {err}"))?;
                if read == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..read]);
            }
            Ok(out)
        })
    }

    pub fn stream_file<F>(&self, path: &str, chunk_size: usize, mut visitor: F) -> Result<u64>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        if path.is_empty() {
            anyhow::bail!("cannot stream FAT volume root as a file");
        }
        let rel = path.trim_start_matches('/').to_string();
        self.with_fs(move |fs| {
            let mut file = fs
                .root_dir()
                .open_file(&rel)
                .map_err(|err| anyhow!("FAT open_file /{rel}: {err}"))?;
            let mut buf = vec![0u8; chunk_size.max(STREAM_FLOOR)];
            let mut total = 0u64;
            loop {
                let read = std::io::Read::read(&mut file, &mut buf)
                    .map_err(|err| anyhow!("FAT stream /{rel}: {err}"))?;
                if read == 0 {
                    break;
                }
                visitor(&buf[..read])?;
                total += read as u64;
            }
            Ok(total)
        })
    }

    pub fn data_runs(&self, path: &str) -> Result<Vec<FsDataRun>> {
        if path.is_empty() {
            return Ok(Vec::new());
        }
        let rel = path.trim_start_matches('/').to_string();
        let base = self.base_offset;
        self.with_fs(move |fs| {
            let mut file = fs
                .root_dir()
                .open_file(&rel)
                .map_err(|err| anyhow!("FAT open_file /{rel}: {err}"))?;
            let mut runs = Vec::new();
            let mut logical = 0u64;
            for extent in file.extents() {
                let extent = extent.map_err(|err| anyhow!("FAT extents /{rel}: {err}"))?;
                let length = u64::from(extent.size);
                runs.push(FsDataRun {
                    logical_offset: logical,
                    physical_offset: base + extent.offset,
                    length,
                });
                logical += length;
            }
            Ok(runs)
        })
    }

    pub fn file_stat(&self, path: &str, node_id: &str) -> Result<FsStat> {
        if path.is_empty() {
            anyhow::bail!("cannot stat FAT volume root as a file");
        }
        let rel = path.trim_start_matches('/').to_string();
        let base = self.base_offset;
        self.with_fs(move |fs| {
            // First locate the directory entry to read length + (later) attrs.
            let parent = parent_dir(&rel);
            let leaf = leaf_name(&rel)?;
            let dir = if parent.is_empty() {
                fs.root_dir()
            } else {
                fs.root_dir()
                    .open_dir(parent)
                    .map_err(|err| anyhow!("FAT open_dir {parent}: {err}"))?
            };
            let mut found = None;
            for entry in dir.iter() {
                let entry = entry.map_err(|err| anyhow!("FAT dir scan in {parent}: {err}"))?;
                if entry.file_name() == leaf {
                    found = Some(entry);
                    break;
                }
            }
            let entry = found.ok_or_else(|| anyhow!("FAT entry not found: /{rel}"))?;
            if entry.is_dir() {
                anyhow::bail!("FAT entry /{rel} is a directory");
            }
            let size = entry.len();

            // Then re-open as File to capture sample + extents (the entry
            // iterator borrows the dir stream so we cannot keep it live).
            drop(entry);
            drop(dir);
            let mut file = fs
                .root_dir()
                .open_file(&rel)
                .map_err(|err| anyhow!("FAT open_file /{rel}: {err}"))?;

            let want = SAMPLE_BYTES.min(size as usize);
            let mut sample = vec![0u8; want];
            let mut filled = 0usize;
            while filled < sample.len() {
                let read = std::io::Read::read(&mut file, &mut sample[filled..])
                    .map_err(|err| anyhow!("FAT stat read /{rel}: {err}"))?;
                if read == 0 {
                    break;
                }
                filled += read;
            }
            sample.truncate(filled);

            let mut runs = Vec::new();
            let mut logical = 0u64;
            for extent in file.extents() {
                let extent = extent.map_err(|err| anyhow!("FAT extents /{rel}: {err}"))?;
                let length = u64::from(extent.size);
                runs.push(FsDataRun {
                    logical_offset: logical,
                    physical_offset: base + extent.offset,
                    length,
                });
                logical += length;
            }

            Ok(FsStat {
                node_id: node_id.to_string(),
                size,
                inode: None,
                mode: None,
                mime: detect_mime_bytes(&sample).to_string(),
                magic_hex: if sample.is_empty() {
                    None
                } else {
                    Some(
                        sample
                            .iter()
                            .take(16)
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<Vec<_>>()
                            .join(" "),
                    )
                },
                entropy: if sample.is_empty() {
                    None
                } else {
                    Some(shannon_entropy(&sample))
                },
                data_runs: runs,
            })
        })
    }

    pub fn root_children(&self) -> Result<Vec<FsNode>> {
        self.children("")
    }

    /// Lists `path`'s children including recovered deleted entries (P1-1). Live
    /// entries come from the vendored crate; deleted `0xE5` slots are decoded
    /// from the directory's raw clusters via [`crate::core::filesystem::volumes::fat_deleted`]
    /// and merged with `deleted: true`, deduped against the live names.
    pub fn children_with_deleted(&self, path: &str) -> Result<Vec<FsNode>> {
        use crate::core::filesystem::volumes::fat_deleted as recover;
        let mut out = self.children(path)?;
        let live: std::collections::HashSet<String> =
            out.iter().map(|n| n.name.to_lowercase()).collect();

        let geom = match recover::open_geom(&self.image, self.base_offset) {
            Ok(g) => g,
            Err(err) => {
                tracing::debug!("FAT recover geom failed: {err}");
                return Ok(out);
            }
        };
        let fat = match recover::FatTable::load(&self.image, &geom) {
            Ok(f) => f,
            Err(err) => {
                tracing::debug!("FAT recover table load failed: {err}");
                return Ok(out);
            }
        };
        let cluster = match recover::resolve_dir_cluster(&self.image, &geom, &fat, path) {
            Ok(c) => c,
            Err(err) => {
                tracing::debug!("FAT recover resolve {path} failed: {err}");
                return Ok(out);
            }
        };
        let dir_bytes = match recover::read_dir_bytes(&self.image, &geom, &fat, cluster) {
            Ok(b) => b,
            Err(err) => {
                tracing::debug!("FAT recover read dir {path} failed: {err}");
                return Ok(out);
            }
        };
        let parent_id = self.node_id(path);
        let mut recovered = recover::scan_dir_deleted(&dir_bytes, &geom);
        // Repair start clusters whose high 16 bits Windows zeroed on delete, so
        // the deleted-entry data run lands on the real bytes instead of garbage deep
        // inside another file's extent (the `_00W.csv` case).
        recover::apply_cluster_reconstruction(&mut recovered, &geom, &fat);
        for entry in recovered {
            if live.contains(&entry.name.to_lowercase()) {
                continue;
            }
            out.push(self.recovered_node(&parent_id, path, entry));
        }
        Ok(out)
    }

    /// Build an `FsNode` for a recovered deleted FAT entry. A deleted file's
    /// FAT chain is zeroed, so its content is reconstructed best-effort as a
    /// single contiguous run; that `(offset, len)` is embedded in the id under
    /// the `deleted:run:` scheme so reads land directly on the image without a
    /// cache. Directories (and entries with no recoverable run) get a
    /// non-readable `deleted:` id. Deleted entries are flagged `deleted: true`.
    fn recovered_node(
        &self,
        parent_id: &str,
        _parent_path: &str,
        entry: crate::core::filesystem::volumes::fat_deleted::RecoveredEntry,
    ) -> FsNode {
        let id = match entry.runs.first() {
            Some(run) if !entry.is_directory && run.length > 0 => format!(
                "deleted:run:part:{}:0x{:x}:0x{:x}",
                self.partition_id, run.physical_offset, run.length
            ),
            _ => format!(
                "deleted:fat:part:{}:{}",
                self.partition_id,
                hex::encode(entry.name.as_bytes())
            ),
        };
        FsNode {
            id,
            name: entry.name,
            parent_id: Some(parent_id.to_string()),
            kind: if entry.is_directory {
                NodeKind::Dir
            } else {
                NodeKind::File
            },
            size: entry.size,
            deleted: true,
            inode: None,
            timestamps: entry.timestamps,
            link_target: None,
            has_snapshot: false,
        }
    }

    pub fn partition_id(&self) -> u8 {
        self.partition_id
    }

    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    /// Unallocated bytes in this volume, from the FAT's free-cluster count
    /// (`free_clusters × cluster_size`). `fatfs::stats()` walks the FAT once to
    /// count free clusters — exact, and cheap relative to a whole-volume scan.
    pub fn free_bytes(&self) -> Result<u64> {
        self.with_fs(|fs| {
            let stats = fs
                .stats()
                .map_err(|err| anyhow!("FAT stats part:{}: {err}", self.partition_id))?;
            Ok(stats.free_clusters() as u64 * stats.cluster_size() as u64)
        })
    }

    fn node_id(&self, path: &str) -> String {
        if path.is_empty() {
            format!("part:{}", self.partition_id)
        } else {
            format!(
                "fat:part:{}:{}",
                self.partition_id,
                hex::encode(path.as_bytes())
            )
        }
    }

    /// Constructs a fresh `fatfs::FileSystem` over the partition slice and
    /// hands it to the closure. The filesystem is dropped immediately after
    /// the closure returns; since our `Write` impl is a no-op (forensic
    /// read-only access) the library's drop-time `unmount_internal` cannot
    /// mutate the underlying image.
    fn with_fs<R>(
        &self,
        op: impl FnOnce(&fatfs::FileSystem<StdIoWrapper<PartitionCursor>>) -> Result<R>,
    ) -> Result<R> {
        let cursor = PartitionCursor {
            image: Arc::clone(&self.image),
            base_offset: self.base_offset,
            length_bytes: self.length_bytes,
            boot_sector_override: self.boot_sector_override,
            position: 0,
        };
        let fs = fatfs::FileSystem::new(cursor, FsOptions::new())
            .map_err(|err| anyhow!("FAT open: {err}"))?;
        op(&fs)
    }
}

fn entry_timestamps<IO, TP, OCC>(entry: &fatfs::DirEntry<'_, IO, TP, OCC>) -> Mactb
where
    IO: fatfs::ReadWriteSeek,
    OCC: fatfs::OemCpConverter,
{
    Mactb {
        modified: fat_datetime_to_rfc3339(entry.modified()),
        accessed: fat_date_to_rfc3339(entry.accessed()),
        created: fat_datetime_to_rfc3339(entry.created()),
        mft_changed: None,
        birth: fat_datetime_to_rfc3339(entry.created()),
    }
}

fn fat_datetime_to_rfc3339(dt: FatDateTime) -> Option<String> {
    let date = NaiveDate::from_ymd_opt(
        i32::from(dt.date.year),
        u32::from(dt.date.month),
        u32::from(dt.date.day),
    )?;
    let naive = date.and_hms_milli_opt(
        u32::from(dt.time.hour),
        u32::from(dt.time.min),
        u32::from(dt.time.sec),
        u32::from(dt.time.millis),
    )?;
    Some(Utc.from_utc_datetime(&naive).to_rfc3339())
}

fn fat_date_to_rfc3339(date: FatDate) -> Option<String> {
    let naive = NaiveDate::from_ymd_opt(
        i32::from(date.year),
        u32::from(date.month),
        u32::from(date.day),
    )?
    .and_hms_opt(0, 0, 0)?;
    Some(Utc.from_utc_datetime(&naive).to_rfc3339())
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() || parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

fn parent_dir(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

fn leaf_name(rel: &str) -> Result<&str> {
    let leaf = rel.rsplit('/').next().unwrap_or("");
    if leaf.is_empty() {
        anyhow::bail!("FAT path has no leaf: /{rel}");
    }
    Ok(leaf)
}

/// Cursor that exposes a partition slice of `DiskImage` as `std::io::Read +
/// Seek + Write`. `Write` is a no-op so any drop-time bookkeeping the FAT
/// library tries to flush silently disappears — our access is read-only by
/// contract, regardless of what the upstream code path believes it is doing.
struct PartitionCursor {
    image: Arc<DiskImage>,
    base_offset: u64,
    length_bytes: u64,
    boot_sector_override: Option<[u8; BOOT_SECTOR_LEN]>,
    position: u64,
}

impl Read for PartitionCursor {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.length_bytes || buf.is_empty() {
            return Ok(0);
        }
        let remaining = self.length_bytes - self.position;
        let want = (buf.len() as u64).min(remaining) as usize;
        let bytes = self
            .image
            .read_range(self.base_offset + self.position, want)
            .map_err(|err| io::Error::other(err.to_string()))?;
        let copied = bytes.len();
        buf[..copied].copy_from_slice(&bytes);
        if let Some(boot_sector) = &self.boot_sector_override {
            overlay_boot_sector(buf, self.position, copied, boot_sector);
        }
        self.position += copied as u64;
        Ok(copied)
    }
}

impl Seek for PartitionCursor {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::Current(delta) => self.position as i128 + delta as i128,
            SeekFrom::End(delta) => self.length_bytes as i128 + delta as i128,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "FAT cursor seek before start",
            ));
        }
        self.position = target as u64;
        Ok(self.position)
    }
}

impl Write for PartitionCursor {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Forensic read-only: pretend the write succeeded so the library's
        // drop-time bookkeeping (dirty flag clear, FS Info flush) cannot
        // touch the underlying image. The image bytes are produced from
        // `read_range`, which returns owned copies, so no aliasing risk.
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn find_fat32_recovered_boot_sector(
    image: &DiskImage,
    base_offset: u64,
    length_bytes: u64,
    sector_size: u64,
) -> Result<Option<[u8; BOOT_SECTOR_LEN]>> {
    if length_bytes < (FAT32_BACKUP_BOOT_SECTOR + 1) * sector_size {
        return Ok(None);
    }

    let primary = image.read_range(base_offset, BOOT_SECTOR_LEN)?;
    if is_valid_fat_boot_sector(&primary, length_bytes) {
        return Ok(None);
    }

    let backup_offset = base_offset + FAT32_BACKUP_BOOT_SECTOR * sector_size;
    let backup = image.read_range(backup_offset, BOOT_SECTOR_LEN)?;
    if !is_valid_fat_boot_sector(&backup, length_bytes) {
        let slice = PartitionSlice::new(image, base_offset, length_bytes);
        return recover_fat32_boot_sector(&slice, length_bytes, sector_size);
    }

    let mut boot_sector = [0u8; BOOT_SECTOR_LEN];
    boot_sector.copy_from_slice(&backup);
    Ok(Some(boot_sector))
}

fn is_valid_fat_boot_sector(bytes: &[u8], volume_size: u64) -> bool {
    if bytes.len() < BOOT_SECTOR_LEN || bytes[510..512] != BOOT_SIGNATURE {
        return false;
    }
    let Some(bytes_per_sector) = read_u16(bytes, 11) else {
        return false;
    };
    if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096) {
        return false;
    }
    let sectors_per_cluster = bytes[13];
    if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
        return false;
    }
    let Some(total_sectors_16) = read_u16(bytes, 19) else {
        return false;
    };
    let Some(total_sectors_32) = read_u32(bytes, 32) else {
        return false;
    };
    let total_sectors = if total_sectors_16 != 0 {
        u32::from(total_sectors_16)
    } else {
        total_sectors_32
    };
    if total_sectors == 0 {
        return false;
    }
    let declared_bytes = u64::from(total_sectors).saturating_mul(u64::from(bytes_per_sector));
    declared_bytes <= volume_size
        && bytes.get(FAT32_LABEL_OFFSET..FAT32_LABEL_OFFSET + FAT32_LABEL_LEN) == Some(b"FAT32   ")
}

fn overlay_boot_sector(buf: &mut [u8], position: u64, copied: usize, boot_sector: &[u8; 512]) {
    let read_start = position;
    let read_end = position.saturating_add(copied as u64);
    if read_start >= BOOT_SECTOR_LEN as u64 || read_end == 0 {
        return;
    }
    let overlap_start = read_start;
    let overlap_end = read_end.min(BOOT_SECTOR_LEN as u64);
    if overlap_start >= overlap_end {
        return;
    }
    let dst_start = (overlap_start - read_start) as usize;
    let dst_end = dst_start + (overlap_end - overlap_start) as usize;
    let src_start = overlap_start as usize;
    let src_end = overlap_end as usize;
    buf[dst_start..dst_end].copy_from_slice(&boot_sector[src_start..src_end]);
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
