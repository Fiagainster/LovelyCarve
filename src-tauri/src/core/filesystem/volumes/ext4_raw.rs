//! Raw on-disk Ext4 helpers for forensic features that ext4-view does not
//! expose — timestamp/dtime extraction, deleted-entry recovery from
//! directory-block slack, physical extent listing for the data-runs view in
//! the hex panel, and the logical piece plan ([`plan_data_pieces`]) shared by
//! the streamed and ranged content readers. Everything here works in raw
//! bytes; the happy path goes through [`ext4_view::Ext4`] in `ext4_volume.rs`.

use chrono::{TimeZone, Utc};

use crate::core::filesystem::tree::fs_tree::{FsDataRun, Mactb, NodeKind};

const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
const EXT4_INLINE_DATA_FL: u32 = 0x1000_0000;
const EXT4_EXTENT_MAGIC: u16 = 0xF30A;

/// Parsed view over the 256-byte raw inode payload returned by
/// [`ext4_view::Ext4::read_raw_inode`]. Keeps the exact field semantics
/// the previous hand-rolled walker exposed so the rest of the project
/// can keep depending on `Ext4Inode` without churn.
#[derive(Debug, Clone)]
pub struct RawInode {
    pub number: u64,
    pub mode: u16,
    pub links_count: u16,
    pub size: u64,
    pub atime: u32,
    pub ctime: u32,
    pub mtime: u32,
    pub dtime: u32,
    pub flags: u32,
    /// `i_blocks_lo`：512 字节单位的占用块数，判定快速符号链接用。
    pub blocks_lo: u32,
    /// `i_file_acl_lo`：扩展属性块指针，非零时要从 `blocks_lo` 里扣除。
    pub file_acl: u32,
    /// `i_file_acl_high`：64-bit 文件系统上的扩展属性块高 16 位。
    pub file_acl_high: u16,
    /// `i_extra_isize`：128-byte 基础 inode 之后的扩展字段长度；内联 xattr
    /// 头紧跟在这段扩展字段之后。
    pub extra_isize: u16,
    pub block: [u8; 60],
}

impl RawInode {
    /// Parse the raw inode bytes returned by `Ext4::read_raw_inode`.
    pub fn parse(inode_number: u64, bytes: &[u8]) -> anyhow::Result<Self> {
        let mode = read_u16(bytes, 0)?;
        let links_count = read_u16(bytes, 26).unwrap_or(0);
        let size_lo = read_u32(bytes, 4)? as u64;
        let size_hi = read_u32(bytes, 108).unwrap_or(0) as u64;
        let flags = read_u32(bytes, 32)?;
        if bytes.len() < 100 {
            anyhow::bail!("raw inode shorter than 100 bytes: {}", bytes.len());
        }
        let mut block = [0u8; 60];
        block.copy_from_slice(&bytes[40..100]);

        Ok(Self {
            number: inode_number,
            mode,
            links_count,
            size: size_lo | (size_hi << 32),
            atime: read_u32(bytes, 8)?,
            ctime: read_u32(bytes, 12)?,
            mtime: read_u32(bytes, 16)?,
            dtime: read_u32(bytes, 20)?,
            flags,
            blocks_lo: read_u32(bytes, 28).unwrap_or(0),
            file_acl: read_u32(bytes, 104).unwrap_or(0),
            file_acl_high: read_u16(bytes, 118).unwrap_or(0),
            extra_isize: read_u16(bytes, 128).unwrap_or(0),
            block,
        })
    }

    pub fn kind(&self) -> NodeKind {
        match self.mode & 0xF000 {
            0x4000 => NodeKind::Dir,
            0x8000 => NodeKind::File,
            0xA000 => NodeKind::Symlink,
            _ => NodeKind::Unallocated,
        }
    }

    pub fn timestamps(&self) -> Mactb {
        Mactb {
            modified: unix_ts(self.mtime),
            accessed: unix_ts(self.atime),
            created: None,
            mft_changed: unix_ts(self.ctime),
            birth: None,
        }
    }

    pub fn uses_extents(&self) -> bool {
        self.flags & EXT4_EXTENTS_FL != 0
            && read_u16(&self.block, 0).unwrap_or(0) == EXT4_EXTENT_MAGIC
    }

    /// 内联数据文件（i_block 里直接存内容，无数据块分配）。
    pub fn is_inline_data(&self) -> bool {
        self.flags & EXT4_INLINE_DATA_FL != 0
    }

    pub fn xattr_block(&self) -> u64 {
        self.file_acl as u64 | ((self.file_acl_high as u64) << 32)
    }
}

/// Single physical extent: a `len_blocks` run of contiguous filesystem
/// blocks starting at `physical_block`, mapped to the file's logical
/// position `logical_block`.
#[derive(Debug, Clone, Copy)]
pub struct PhysExtent {
    pub logical_block: u32,
    pub physical_block: u64,
    pub len_blocks: u16,
}

/// Walk the extent tree rooted at the inode's i_block area. Returns the
/// flattened list of leaf extents. Tree depth is capped at 5 to keep
/// pathological images from looping forever.
pub fn extents_from_tree<F>(
    node: &[u8],
    depth: u16,
    block_size: u64,
    mut load_block: F,
) -> anyhow::Result<Vec<PhysExtent>>
where
    F: FnMut(u64) -> anyhow::Result<Vec<u8>>,
{
    extents_from_tree_inner(node, depth, block_size, &mut load_block)
}

fn extents_from_tree_inner<F>(
    node: &[u8],
    depth: u16,
    block_size: u64,
    load_block: &mut F,
) -> anyhow::Result<Vec<PhysExtent>>
where
    F: FnMut(u64) -> anyhow::Result<Vec<u8>>,
{
    if depth > 5 {
        anyhow::bail!("ext4 extent tree is too deep");
    }
    if read_u16(node, 0)? != EXT4_EXTENT_MAGIC {
        anyhow::bail!("invalid ext4 extent header");
    }
    let entries = read_u16(node, 2)? as usize;
    let tree_depth = read_u16(node, 6)?;
    let mut out = Vec::new();

    if tree_depth == 0 {
        for i in 0..entries {
            let off = 12 + i * 12;
            if off + 12 > node.len() {
                break;
            }
            let len_raw = read_u16(node, off + 4)?;
            out.push(PhysExtent {
                logical_block: read_u32(node, off)?,
                physical_block: ((read_u16(node, off + 6)? as u64) << 32)
                    | read_u32(node, off + 8)? as u64,
                len_blocks: len_raw & 0x7FFF,
            });
        }
    } else {
        for i in 0..entries {
            let off = 12 + i * 12;
            if off + 12 > node.len() {
                break;
            }
            let leaf = read_u32(node, off + 4)? as u64 | ((read_u16(node, off + 8)? as u64) << 32);
            let child = load_block(leaf * block_size)?;
            out.extend(extents_from_tree_inner(
                &child,
                depth + 1,
                block_size,
                load_block,
            )?);
        }
    }

    Ok(out)
}

/// Build the visible-region data runs reported through the hex panel.
/// Output is in logical order. `physical_offset` is absolute within the
/// disk image, so the hex view can read directly without going back
/// through the volume layer.
pub fn data_runs_from_inode<F>(
    inode: &RawInode,
    base_offset: u64,
    block_size: u64,
    mut load_block: F,
) -> anyhow::Result<Vec<FsDataRun>>
where
    F: FnMut(u64) -> anyhow::Result<Vec<u8>>,
{
    if inode.size == 0 {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();
    if inode.uses_extents() {
        for extent in extents_from_tree_inner(&inode.block, 0, block_size, &mut load_block)? {
            let logical_offset = extent.logical_block as u64 * block_size;
            if logical_offset >= inode.size {
                continue;
            }
            let extent_len = extent.len_blocks as u64 * block_size;
            runs.push(FsDataRun {
                logical_offset,
                physical_offset: base_offset + extent.physical_block * block_size,
                length: extent_len.min(inode.size - logical_offset),
            });
        }
    } else {
        for i in 0..12 {
            let block_ptr = read_u32(&inode.block, i * 4)? as u64;
            if block_ptr == 0 {
                continue;
            }
            let logical_offset = i as u64 * block_size;
            if logical_offset >= inode.size {
                break;
            }
            runs.push(FsDataRun {
                logical_offset,
                physical_offset: base_offset + block_ptr * block_size,
                length: block_size.min(inode.size - logical_offset),
            });
        }
    }

    runs.sort_by_key(|run| run.logical_offset);
    Ok(runs)
}

/// One recovered slack-space directory entry. The caller turns this into
/// an `FsNode` — we keep the type plain here so the forensic logic stays
/// independent of the node format.
#[derive(Debug, Clone)]
pub struct SlackEntry {
    pub child_inode: u64,
    pub name: String,
    pub file_type: u8,
    pub slot_offset: usize,
}

/// Walk a single directory block scanning the gap between
/// `align4(8 + name_len)` and `rec_len` for partially-overwritten entries
/// of deleted children. Mirrors the behaviour of the original walker
/// (best-effort, name-validating, dedup via `seen`).
pub fn recover_slack_entries(
    block: &[u8],
    block_size: usize,
    seen: &mut std::collections::HashSet<(u64, String)>,
) -> Vec<SlackEntry> {
    let mut out = Vec::new();
    for chunk in block.chunks(block_size) {
        let mut pos = 0usize;
        while pos + 8 <= chunk.len() {
            let rec_len = match chunk.get(pos + 4..pos + 6) {
                Some(&[a, b]) => u16::from_le_bytes([a, b]) as usize,
                _ => break,
            };
            let name_len = chunk[pos + 6] as usize;
            if rec_len < 8 || pos + rec_len > chunk.len() {
                break;
            }

            let actual_len = align4(8 + name_len);
            if rec_len > actual_len + 8 {
                slack_walk(chunk, pos + actual_len, pos + rec_len, seen, &mut out);
            }

            pos += rec_len;
        }
    }
    out
}

fn slack_walk(
    block: &[u8],
    start: usize,
    end: usize,
    seen: &mut std::collections::HashSet<(u64, String)>,
    out: &mut Vec<SlackEntry>,
) {
    let mut cursor = start;
    while cursor + 8 <= end && cursor + 8 <= block.len() {
        let child_inode = u32::from_le_bytes([
            block[cursor],
            block[cursor + 1],
            block[cursor + 2],
            block[cursor + 3],
        ]) as u64;
        let rec_len = u16::from_le_bytes([block[cursor + 4], block[cursor + 5]]) as usize;
        let name_len = block[cursor + 6] as usize;
        let file_type = block[cursor + 7];

        if child_inode == 0
            || rec_len < 8
            || cursor + rec_len > end
            || name_len == 0
            || name_len > rec_len.saturating_sub(8)
            || !matches!(file_type, 1 | 2 | 7)
        {
            cursor += 4;
            continue;
        }

        let raw_name = &block[cursor + 8..cursor + 8 + name_len];
        if !looks_like_filename(raw_name) {
            cursor += 4;
            continue;
        }
        let name = String::from_utf8_lossy(raw_name).to_string();
        if name == "." || name == ".." || !seen.insert((child_inode, format!("deleted:{name}"))) {
            cursor += rec_len.max(4);
            continue;
        }

        out.push(SlackEntry {
            child_inode,
            name,
            file_type,
            slot_offset: cursor,
        });
        cursor += rec_len.max(4);
    }
}

fn unix_ts(value: u32) -> Option<String> {
    if value == 0 {
        None
    } else {
        Utc.timestamp_opt(value as i64, 0)
            .single()
            .map(|dt| dt.to_rfc3339())
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> anyhow::Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| anyhow::anyhow!("short ext4 read"))?
            .try_into()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> anyhow::Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow::anyhow!("short ext4 read"))?
            .try_into()?,
    ))
}

fn align4(value: usize) -> usize {
    (value + 3) & !3
}

fn looks_like_filename(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= 255
        && bytes.iter().all(|byte| {
            matches!(*byte, 0x20..=0x7e | 0x80..=0xff) && *byte != b'/' && *byte != 0x7f
        })
}

/// 一片连续的逻辑字节：空洞（读零），或镜像绝对偏移 `physical` 起的数据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DataPiece {
    Zero(u64),
    Data { physical: u64, len: u64 },
}

impl DataPiece {
    pub(crate) fn len(self) -> u64 {
        match self {
            Self::Zero(len) | Self::Data { len, .. } => len,
        }
    }
}

/// 把 extent 数据段展开成首尾相接、总长恰为 `size` 的逻辑字节片。流式读与随机读
/// 共用这一份映射，两条读路径因此逐字节一致：
///
/// - 段间与尾部空洞读零；
/// - 越过镜像末端（`image_len`）的段只保留镜像内的部分，缺口由其后的空洞补零；
/// - 段按逻辑起点顺序接续，重叠段（损坏的 extent 树）不回退逻辑位置，紧接在已
///   产出的字节之后——与流式读一直以来的推进方式相同。
pub(crate) fn plan_data_pieces(runs: &[FsDataRun], size: u64, image_len: u64) -> Vec<DataPiece> {
    let mut pieces = Vec::with_capacity(runs.len() * 2 + 1);
    let mut logical = 0u64;
    for run in runs {
        if run.logical_offset > logical {
            let hole = (run.logical_offset - logical).min(size - logical);
            if hole > 0 {
                pieces.push(DataPiece::Zero(hole));
            }
            logical += hole;
        }
        if logical >= size {
            break;
        }
        let len = run
            .length
            .min(size - logical)
            .min(image_len.saturating_sub(run.physical_offset));
        if len > 0 {
            pieces.push(DataPiece::Data {
                physical: run.physical_offset,
                len,
            });
            logical += len;
        }
    }
    if logical < size {
        pieces.push(DataPiece::Zero(size - logical));
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(logical_offset: u64, physical_offset: u64, length: u64) -> FsDataRun {
        FsDataRun {
            logical_offset,
            physical_offset,
            length,
        }
    }

    #[test]
    fn plan_fills_holes_between_runs_and_at_the_tail() {
        let runs = [run(4096, 1_000_000, 4096), run(16384, 2_000_000, 4096)];
        assert_eq!(
            plan_data_pieces(&runs, 24_000, u64::MAX),
            vec![
                DataPiece::Zero(4096),
                DataPiece::Data {
                    physical: 1_000_000,
                    len: 4096
                },
                DataPiece::Zero(8192),
                DataPiece::Data {
                    physical: 2_000_000,
                    len: 4096
                },
                DataPiece::Zero(3520),
            ]
        );
    }

    #[test]
    fn plan_clips_the_last_run_to_the_file_size() {
        let runs = [run(0, 8192, 8192)];
        assert_eq!(
            plan_data_pieces(&runs, 5000, u64::MAX),
            vec![DataPiece::Data {
                physical: 8192,
                len: 5000
            }]
        );
    }

    #[test]
    fn plan_zero_fills_bytes_past_the_image_end() {
        // 第一段有一半落在镜像外：可读部分照读，缺口由下一段前的空洞补零，
        // 后续段的逻辑位置不受影响。
        let runs = [run(0, 6000, 4096), run(8192, 100, 4096)];
        assert_eq!(
            plan_data_pieces(&runs, 12_288, 8000),
            vec![
                DataPiece::Data {
                    physical: 6000,
                    len: 2000
                },
                DataPiece::Zero(6192),
                DataPiece::Data {
                    physical: 100,
                    len: 4096
                },
            ]
        );
    }

    #[test]
    fn plan_continues_overlapping_runs_after_emitted_bytes() {
        // 损坏 extent 树的重叠段不回退：第二段紧接在第一段产出的字节之后。
        let runs = [run(0, 1000, 4096), run(2048, 9000, 4096)];
        assert_eq!(
            plan_data_pieces(&runs, 10_000, u64::MAX),
            vec![
                DataPiece::Data {
                    physical: 1000,
                    len: 4096
                },
                DataPiece::Data {
                    physical: 9000,
                    len: 4096
                },
                DataPiece::Zero(1808),
            ]
        );
    }

    #[test]
    fn plan_of_a_run_less_file_is_one_hole() {
        assert_eq!(
            plan_data_pieces(&[], 4096, u64::MAX),
            vec![DataPiece::Zero(4096)]
        );
        assert_eq!(plan_data_pieces(&[], 0, u64::MAX), Vec::new());
    }
}
