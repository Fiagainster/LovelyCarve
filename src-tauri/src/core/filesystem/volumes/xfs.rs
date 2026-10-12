//! Read-only XFS volume adapter built on the vendored `xfs-fuse` crate.
//!
//! The `xfs_fuse::Volume` is built once in [`XfsVolume::open`] and reused for
//! every operation behind a `Mutex` (the crate's reads take `&mut`). Re-
//! parsing the superblock + root inode costs ~5–15 ms on a cold image, so a
//! whole-volume snapshot walk would otherwise pay that per directory; holding
//! the handle (and caching the `XfsVolume` itself in
//! [`crate::core::filesystem::tree::fs_tree::FsSession`]) keeps it to one parse per walk.
//! Live IPC calls still open their own `XfsVolume`, so the mutex is
//! uncontended outside the single-threaded walker.
//!
//! Node IDs encode the absolute UTF-8 path inside the volume as hex bytes:
//! `xfs:part:{N}:{hex(/path/to/entry)}`. The empty-hex form denotes the
//! volume root and is never emitted directly — the partition node itself
//! stands in for the root.
//!
//! The vendored `xfs-fuse` originally kept the active superblock in a
//! process-global slot because a few `Decode` impls reach for `sb.has_ftype()`
//! / `sb.sb_uuid` during bincode decoding. LovelyCarve now binds that slot per
//! thread before each volume operation, so a boot XFS partition and an LVM XFS
//! root volume with different UUIDs can be browsed in one process.

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use xfs_fuse::{FileType, Volume, XfsIno};

use crate::core::evidence::image::find_partition;
use crate::core::evidence::image::{DiskImage, Partition};
use crate::core::filesystem::tree::fs_tree::{
    detect_mime_bytes, shannon_entropy, FsDataRun, FsNode, FsStat, Mactb, NodeKind,
};
use crate::core::filesystem::volumes::lvm::LvmVolume;

const SAMPLE_BYTES: usize = 4096;
const STREAM_FLOOR: usize = 64 * 1024;
/// Cap how much of a directory we surface to the UI in one listing. XFS
/// directories scale far beyond this on real filesystems (think `cur/` for a
/// large maildir), but past this the UI becomes unusable; pagination can be
/// added later.
const MAX_CHILDREN_PER_DIR: usize = 50_000;

#[derive(Debug, Clone, Copy)]
pub enum XfsSource {
    Partition(u8),
    Lvm { part_id: u8, lv_index: usize },
}

/// Parse a `xfs:part:{N}:{hex(path)}` or `xfs:lvm:{N}:{M}:{hex(path)}` ID.
/// Returns the source plus the decoded UTF-8 path (with a leading `/`, or
/// empty for the volume root).
pub fn parse_xfs_id(id: &str) -> Option<(XfsSource, String)> {
    let mut parts = id.split(':');
    match parts.next()? {
        "xfs" => {}
        _ => return None,
    }
    match parts.next()? {
        "part" => {
            let part_id = parts.next()?.parse().ok()?;
            let hex_path = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let path = decode_hex_path(hex_path)?;
            Some((XfsSource::Partition(part_id), path))
        }
        "lvm" => {
            let part_id = parts.next()?.parse().ok()?;
            let lv_index = parts.next()?.parse().ok()?;
            let hex_path = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let path = decode_hex_path(hex_path)?;
            Some((XfsSource::Lvm { part_id, lv_index }, path))
        }
        _ => None,
    }
}

fn decode_hex_path(hex_path: &str) -> Option<String> {
    if hex_path.is_empty() {
        Some(String::new())
    } else {
        let bytes = hex::decode(hex_path).ok()?;
        String::from_utf8(bytes).ok()
    }
}

pub fn open_xfs_source(image: &Arc<DiskImage>, source: XfsSource) -> Result<XfsVolume> {
    match source {
        XfsSource::Partition(part_id) => {
            let partition = find_partition(image, &part_id.to_string())?;
            let sector_size = image.sector_size as u64;
            XfsVolume::open(
                Arc::clone(image),
                source,
                partition.start_lba * sector_size,
                partition.length_lba * sector_size,
                format!("part:{part_id}"),
            )
        }
        XfsSource::Lvm { part_id, lv_index } => {
            let partition = find_partition(image, &part_id.to_string())?;
            let lvm = LvmVolume::open(image, partition)?;
            let lv = lvm
                .logical_volumes
                .get(lv_index)
                .ok_or_else(|| anyhow!("logical volume not found: lvm:{part_id}:{lv_index}"))?;
            XfsVolume::open(
                Arc::clone(image),
                source,
                lv.absolute_offset,
                lv.size_bytes,
                format!("lvm:{part_id}:{lv_index}"),
            )
        }
    }
}

pub struct XfsVolume {
    source: XfsSource,
    root_parent_id: String,
    /// Parsed `xfs_fuse::Volume`, built once in [`XfsVolume::open`] and reused
    /// behind a mutex (the crate's reads take `&mut`). See the module header.
    volume: Mutex<Volume<PartitionCursor>>,
}

impl XfsVolume {
    pub fn open(
        image: Arc<DiskImage>,
        source: XfsSource,
        base_offset: u64,
        length_bytes: u64,
        root_parent_id: String,
    ) -> Result<Self> {
        let cursor = PartitionCursor {
            image,
            base_offset,
            length_bytes,
            position: 0,
        };
        // `Volume::from` is infallible (xfs_fuse parses lazily on first
        // access); errors surface on the first listing, same as before.
        let volume = Volume::from(cursor);
        Ok(Self {
            source,
            root_parent_id,
            volume: Mutex::new(volume),
        })
    }

    pub fn open_partition(image: Arc<DiskImage>, partition: &Partition) -> Result<Self> {
        let sector_size = image.sector_size as u64;
        Self::open(
            image,
            XfsSource::Partition(partition.id),
            partition.start_lba * sector_size,
            partition.length_lba * sector_size,
            format!("part:{}", partition.id),
        )
    }

    pub fn root_children(&self) -> Result<Vec<FsNode>> {
        self.children("")
    }

    /// Lists the direct children of `path` inside the volume. `path` uses
    /// Unix-style separators with a leading `/`, or is empty for the volume
    /// root.
    pub fn children(&self, path: &str) -> Result<Vec<FsNode>> {
        let parent_id = self.node_id(path);
        let parent_path = path.to_string();
        self.with_volume(|vol| {
            let parent_ino = lookup_path_resilient(vol, path)
                .map_err(|err| anyhow!("XFS lookup_path {path}: errno {err}"))?;
            let entries = vol
                .readdir(parent_ino)
                .map_err(|err| anyhow!("XFS readdir {path}: errno {err}"))?;

            let mut out = Vec::with_capacity(entries.len());
            for entry in entries.into_iter().take(MAX_CHILDREN_PER_DIR) {
                if entry.name == b"." || entry.name == b".." {
                    continue;
                }
                let name = String::from_utf8_lossy(&entry.name).into_owned();
                let attr = match vol.getattr(entry.ino) {
                    Ok(attr) => attr,
                    Err(err) => {
                        tracing::debug!(
                            "skip unreadable XFS entry {}/{} ino={} errno={err}",
                            parent_path,
                            name,
                            entry.ino
                        );
                        continue;
                    }
                };
                let kind = node_kind_from_filetype(entry.kind.unwrap_or(attr.kind));
                let size = attr.size;
                let timestamps = attr_timestamps(&attr);
                let child_path = join_unix(&parent_path, &name);
                out.push(FsNode {
                    id: self.node_id(&child_path),
                    name,
                    parent_id: Some(parent_id.clone()),
                    kind,
                    size,
                    deleted: false,
                    inode: Some(entry.ino),
                    timestamps,
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
            anyhow::bail!("cannot read XFS volume root as a file");
        }
        let owned = path.to_string();
        self.with_volume(|vol| {
            let ino = lookup_path_resilient(vol, &owned)
                .map_err(|err| anyhow!("XFS lookup_path {owned}: errno {err}"))?;
            let attr = vol
                .getattr(ino)
                .map_err(|err| anyhow!("XFS getattr {owned}: errno {err}"))?;
            match attr.kind {
                FileType::RegularFile => read_file_chunks(vol, ino, attr.size, max_bytes),
                FileType::Symlink => vol
                    .readlink(ino)
                    .map_err(|err| anyhow!("XFS readlink {owned}: errno {err}")),
                _ => anyhow::bail!("XFS entry {owned} is not a regular file"),
            }
        })
    }

    pub fn stream_file<F>(&self, path: &str, chunk_size: usize, mut visitor: F) -> Result<u64>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        if path.is_empty() {
            anyhow::bail!("cannot stream XFS volume root as a file");
        }
        let owned = path.to_string();
        self.with_volume(|vol| {
            let ino = lookup_path_resilient(vol, &owned)
                .map_err(|err| anyhow!("XFS lookup_path {owned}: errno {err}"))?;
            let attr = vol
                .getattr(ino)
                .map_err(|err| anyhow!("XFS getattr {owned}: errno {err}"))?;
            if attr.kind != FileType::RegularFile {
                anyhow::bail!("XFS entry {owned} is not a regular file");
            }
            let chunk = chunk_size.max(STREAM_FLOOR);
            let mut offset: u64 = 0;
            while offset < attr.size {
                let want = ((attr.size - offset) as usize)
                    .min(chunk)
                    .min(u32::MAX as usize) as u32;
                let bytes = vol
                    .read(ino, offset as i64, want)
                    .map_err(|err| anyhow!("XFS read {owned}@{offset}: errno {err}"))?;
                if bytes.is_empty() {
                    break;
                }
                visitor(&bytes)?;
                offset += bytes.len() as u64;
            }
            Ok(offset)
        })
    }

    pub fn file_stat(&self, path: &str, node_id: &str) -> Result<FsStat> {
        if path.is_empty() {
            anyhow::bail!("cannot stat XFS volume root as a file");
        }
        let owned = path.to_string();
        self.with_volume(|vol| {
            let ino = lookup_path_resilient(vol, &owned)
                .map_err(|err| anyhow!("XFS lookup_path {owned}: errno {err}"))?;
            let attr = vol
                .getattr(ino)
                .map_err(|err| anyhow!("XFS getattr {owned}: errno {err}"))?;
            let mut sample = Vec::new();
            match attr.kind {
                FileType::RegularFile => {
                    let want = (SAMPLE_BYTES as u64).min(attr.size) as u32;
                    if want > 0 {
                        sample = vol.read(ino, 0, want).unwrap_or_default();
                    }
                }
                FileType::Symlink => {
                    if let Ok(link) = vol.readlink(ino) {
                        sample = link;
                    }
                }
                _ => {}
            }
            Ok(FsStat {
                node_id: node_id.to_string(),
                size: attr.size,
                inode: Some(ino),
                mode: Some(attr.perm),
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
                // XFS extents could be surfaced via the bmap walker, but the
                // vendored library doesn't expose a public extent iterator.
                // Leaving empty until a future patch adds one.
                data_runs: Vec::<FsDataRun>::new(),
            })
        })
    }

    fn node_id(&self, path: &str) -> String {
        if path.is_empty() {
            return self.root_parent_id.clone();
        }
        let hex = hex::encode(path.as_bytes());
        match self.source {
            XfsSource::Partition(part_id) => format!("xfs:part:{part_id}:{hex}"),
            XfsSource::Lvm { part_id, lv_index } => {
                format!("xfs:lvm:{part_id}:{lv_index}:{hex}")
            }
        }
    }

    /// Runs `op` against the parsed `xfs_fuse::Volume` held by this volume.
    /// Built once in [`XfsVolume::open`]; reads address absolute offsets, so
    /// reusing one cursor across calls is safe for read-only access. A
    /// poisoned mutex (a prior panic mid-read) surfaces as an error.
    fn with_volume<R>(
        &self,
        op: impl FnOnce(&mut Volume<PartitionCursor>) -> Result<R>,
    ) -> Result<R> {
        let mut volume = self
            .volume
            .lock()
            .map_err(|_| anyhow!("XFS volume mutex poisoned"))?;
        op(&mut volume)
    }
}

fn lookup_path_resilient(
    vol: &mut Volume<PartitionCursor>,
    path: &str,
) -> std::result::Result<XfsIno, i32> {
    match vol.lookup_path(path) {
        Ok(ino) => Ok(ino),
        Err(fast_error) => {
            let fallback = lookup_path_by_readdir(vol, path);
            if fallback.is_ok() {
                tracing::debug!(
                    "XFS lookup_path {path} failed with errno {fast_error}; resolved via readdir"
                );
            }
            fallback.map_err(|_| fast_error)
        }
    }
}

fn lookup_path_by_readdir(
    vol: &mut Volume<PartitionCursor>,
    path: &str,
) -> std::result::Result<XfsIno, i32> {
    let mut current = vol.lookup_path("")?;
    for component in path.split('/').filter(|component| !component.is_empty()) {
        let target = component.as_bytes();
        let entries = vol.readdir(current)?;
        let Some(entry) = entries.into_iter().find(|entry| entry.name == target) else {
            return vol.lookup_path(path);
        };
        current = entry.ino;
    }
    Ok(current)
}

fn read_file_chunks(
    vol: &mut Volume<PartitionCursor>,
    ino: XfsIno,
    size: u64,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    let total = (size as usize).min(max_bytes);
    let mut out = Vec::with_capacity(total);
    let chunk: usize = 1 << 20; // 1 MiB
    while out.len() < total {
        let want = (total - out.len()).min(chunk).min(u32::MAX as usize) as u32;
        let bytes = vol
            .read(ino, out.len() as i64, want)
            .map_err(|err| anyhow!("XFS read errno {err}"))?;
        if bytes.is_empty() {
            break;
        }
        out.extend_from_slice(&bytes);
    }
    out.truncate(total);
    Ok(out)
}

fn node_kind_from_filetype(kind: FileType) -> NodeKind {
    match kind {
        FileType::Directory => NodeKind::Dir,
        FileType::Symlink => NodeKind::Symlink,
        _ => NodeKind::File,
    }
}

fn attr_timestamps(attr: &xfs_fuse::FileAttr) -> Mactb {
    Mactb {
        modified: system_time_to_rfc3339(attr.mtime),
        accessed: system_time_to_rfc3339(attr.atime),
        created: system_time_to_rfc3339(attr.crtime),
        mft_changed: system_time_to_rfc3339(attr.ctime),
        birth: system_time_to_rfc3339(attr.crtime),
    }
}

fn system_time_to_rfc3339(t: std::time::SystemTime) -> Option<String> {
    DateTime::<Utc>::from(t).to_rfc3339().into()
}

fn join_unix(parent: &str, name: &str) -> String {
    if parent.is_empty() || parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

/// Cursor that exposes a partition slice of `DiskImage` as `std::io::Read +
/// Seek`. Read-only by contract; the inner `image.read_range` returns owned
/// copies, so there's no aliasing risk.
pub struct PartitionCursor {
    image: Arc<DiskImage>,
    base_offset: u64,
    length_bytes: u64,
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
                "XFS cursor seek before start",
            ));
        }
        self.position = target as u64;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::evidence::image::PartitionKind;

    /// Drive the adapter against a real XFS image when the path is supplied
    /// via `LOVELYCARVE_XFS_TEST`. Walks the root, samples the first regular
    /// file, and confirms `read_file` returns bytes.
    #[test]
    fn lists_real_xfs_root_when_env_path_is_set() {
        let Some(path) = std::env::var_os("LOVELYCARVE_XFS_TEST") else {
            return;
        };
        let image = Arc::new(
            crate::core::filesystem::evidence_session::EvidenceSession::open(path)
                .expect("XFS image should open"),
        );
        let xfs_partition = image
            .partitions
            .iter()
            .find(|p| p.kind == PartitionKind::Xfs)
            .cloned()
            .expect("expected at least one XFS partition");
        let volume =
            XfsVolume::open_partition(Arc::clone(&image), &xfs_partition).expect("open XFS");
        let children = volume.root_children().expect("list XFS root");
        assert!(!children.is_empty(), "XFS root should expose entries");
        if let Some(first_file) = children.iter().find(|node| node.kind == NodeKind::File) {
            let path = format!("/{}", first_file.name);
            let bytes = volume.read_file(&path, 8192).expect("XFS file read");
            assert!(!bytes.is_empty(), "XFS file should yield bytes");
        }
    }

    /// Regression guard for the xfs-fuse panic on freed/unsupported inodes.
    /// A directory entry that points at a zeroed inode (e.g. a runtime
    /// `/var/tmp/systemd-private-*` dir on a real CentOS image) used to abort
    /// the whole walk with `panic!("Inode type (0o0) not yet supported.")`
    /// inside `getattr`; it must now degrade to an `Err`-and-skip so a
    /// volume-snapshot / tree browse completes. Drives the live dispatcher
    /// (the exact path the GUI takes), bounded so a large volume still
    /// finishes the check quickly. Env-gated on `LOVELYCARVE_XFS_TEST`.
    #[test]
    fn walks_real_xfs_tree_without_panicking_when_env_path_is_set() {
        let Some(path) = std::env::var_os("LOVELYCARVE_XFS_TEST") else {
            return;
        };
        let image = Arc::new(
            crate::core::filesystem::evidence_session::EvidenceSession::open(path)
                .expect("XFS image should open"),
        );
        let xfs_partition = image
            .partitions
            .iter()
            .find(|p| p.kind == PartitionKind::Xfs)
            .cloned()
            .expect("expected at least one XFS partition");

        let mut stack = vec![format!("part:{}", xfs_partition.id)];
        let mut visited = std::collections::HashSet::new();
        let mut count = 0usize;
        let mut skipped_dirs = 0usize;
        while let Some(id) = stack.pop() {
            if count >= 200_000 {
                break;
            }
            // A dir whose own inode can't be parsed returns Err here (no panic);
            // skip it and keep walking, exactly as the snapshot builder does.
            let children =
                match crate::core::filesystem::tree::fs_tree::children_for_image(&image, &id) {
                    Ok(children) => children,
                    Err(_) => {
                        skipped_dirs += 1;
                        continue;
                    }
                };
            for child in children {
                count += 1;
                if child.kind == NodeKind::Dir && visited.insert(child.id.clone()) {
                    stack.push(child.id);
                }
            }
        }
        assert!(
            count > 100,
            "should enumerate a non-trivial XFS tree without panicking, saw {count}"
        );
        let _ = skipped_dirs;
    }

    #[test]
    fn reads_real_lvm_xfs_after_plain_xfs_when_env_path_is_set() {
        let Some(path) = std::env::var_os("LOVELYCARVE_XFS_LVM_TEST") else {
            return;
        };
        let image = Arc::new(
            crate::core::filesystem::evidence_session::EvidenceSession::open(path)
                .expect("XFS image should open"),
        );
        let xfs_partition = image
            .partitions
            .iter()
            .find(|partition| partition.kind == PartitionKind::Xfs)
            .cloned()
            .expect("expected a plain XFS partition");
        let boot_volume =
            XfsVolume::open_partition(Arc::clone(&image), &xfs_partition).expect("open boot XFS");
        assert!(
            !boot_volume
                .root_children()
                .expect("list boot XFS root")
                .is_empty(),
            "boot XFS root should expose entries"
        );

        let lvm_partition = image
            .partitions
            .iter()
            .find(|partition| partition.kind == PartitionKind::Lvm)
            .cloned()
            .expect("expected an LVM partition");
        let lvm = LvmVolume::open(&image, &lvm_partition).expect("open LVM");
        let mut root_volume = None;
        for logical_volume in lvm
            .logical_volumes
            .iter()
            .filter(|volume| volume.kind == PartitionKind::Xfs)
        {
            let volume = open_xfs_source(
                &image,
                XfsSource::Lvm {
                    part_id: lvm_partition.id,
                    lv_index: logical_volume.index,
                },
            )
            .expect("open LVM XFS volume");
            let root = volume.root_children().expect("list LVM XFS root");
            if root
                .iter()
                .any(|node| node.kind == NodeKind::Dir && node.name == "etc")
            {
                root_volume = Some(volume);
                break;
            }
        }
        let volume = root_volume.expect("expected an LVM XFS root containing /etc");
        let etc = volume.children("/etc").expect("list LVM XFS /etc");
        assert!(!etc.is_empty(), "/etc should expose entries");
        if let Some(first_file) = etc
            .iter()
            .find(|node| node.kind == NodeKind::File && node.size > 0 && node.size < 8192)
        {
            let path = format!("/etc/{}", first_file.name);
            let bytes = volume
                .read_file(&path, 8192)
                .unwrap_or_else(|err| panic!("read {path} through LVM XFS: {err}"));
            assert!(!bytes.is_empty(), "{path} should yield bytes");
        }
    }
}
