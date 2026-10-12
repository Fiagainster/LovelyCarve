//! Read-only Ext4 volume backed by the vendored `ext4-view` crate.
//!
//! `Ext4Volume` keeps the inode-indexed public API the rest of the
//! project relies on (the FsNode tree IDs are `ext4:part:N:inode`),
//! while delegating block extent walking, htree directory lookups,
//! journal replay, inline data, and metadata checksum validation to
//! `ext4_view::Ext4`. The forensic features that `ext4-view` does not
//! cover — timestamp/dtime extraction, deleted-entry recovery from
//! directory slack, physical data-run listing for the hex panel — sit
//! in [`crate::core::filesystem::volumes::ext4_raw`].

use std::collections::HashSet;
use std::sync::Arc;

use ext4_view::{Ext4, Ext4Read, FileType};

use crate::core::evidence::image::find_partition;
use crate::core::evidence::image::DiskImage;
use crate::core::filesystem::tree::fs_tree::{FsDataRun, FsNode, NodeKind};
use crate::core::filesystem::volumes::ext4_raw::{
    data_runs_from_inode, plan_data_pieces, recover_slack_entries, DataPiece, RawInode, SlackEntry,
};
use crate::core::filesystem::volumes::ext4_xattr::{
    parse_inode_xattrs, parse_xattr_block, Ext4Xattr,
};
use crate::core::filesystem::volumes::lvm::LvmVolume;

pub type Ext4Inode = RawInode;

#[derive(Debug, Clone, Copy)]
pub enum Ext4Source {
    Partition(u8),
    Lvm { part_id: u8, lv_index: usize },
}

pub fn parse_ext4_id(id: &str) -> Option<(Ext4Source, u64)> {
    let mut parts = id.split(':');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("ext4"), Some("part"), Some(part_id), Some(inode), None, None) => Some((
            Ext4Source::Partition(part_id.parse().ok()?),
            inode.parse().ok()?,
        )),
        (Some("ext4"), Some("lvm"), Some(part_id), Some(lv_index), Some(inode), None) => Some((
            Ext4Source::Lvm {
                part_id: part_id.parse().ok()?,
                lv_index: lv_index.parse().ok()?,
            },
            inode.parse().ok()?,
        )),
        _ => None,
    }
}

pub fn canonical_ext4_id(id: &str) -> String {
    let parts = id.split(':').collect::<Vec<_>>();
    match parts.as_slice() {
        ["recover", "ext4", "part", part_id, inode, ..] => {
            format!("ext4:part:{part_id}:{inode}")
        }
        ["recover", "ext4", "lvm", part_id, lv_index, inode, ..] => {
            format!("ext4:lvm:{part_id}:{lv_index}:{inode}")
        }
        _ => id.to_string(),
    }
}

pub fn open_ext4_source(image: &Arc<DiskImage>, source: Ext4Source) -> anyhow::Result<Ext4Volume> {
    match source {
        Ext4Source::Partition(part_id) => {
            let partition = find_partition(image, &part_id.to_string())?;
            Ext4Volume::open(
                Arc::clone(image),
                source,
                partition.start_lba * image.sector_size as u64,
                partition.length_lba * image.sector_size as u64,
                format!("part:{part_id}"),
            )
        }
        Ext4Source::Lvm { part_id, lv_index } => {
            let partition = find_partition(image, &part_id.to_string())?;
            let lvm = LvmVolume::open(image, partition)?;
            let lv = lvm.logical_volumes.get(lv_index).ok_or_else(|| {
                anyhow::anyhow!("logical volume not found: lvm:{part_id}:{lv_index}")
            })?;
            Ext4Volume::open(
                Arc::clone(image),
                source,
                lv.absolute_offset,
                lv.size_bytes,
                format!("lvm:{part_id}:{lv_index}"),
            )
        }
    }
}

#[derive(Debug)]
pub struct Ext4Volume {
    image: Arc<DiskImage>,
    fs: Ext4,
    source: Ext4Source,
    root_parent_id: String,
    base_offset: u64,
    length_bytes: u64,
    block_size: u64,
}

impl Ext4Volume {
    pub fn open(
        image: Arc<DiskImage>,
        source: Ext4Source,
        base_offset: u64,
        length_bytes: u64,
        root_parent_id: String,
    ) -> anyhow::Result<Self> {
        let reader = ImageSliceReader {
            image: Arc::clone(&image),
            base_offset,
            length_bytes,
        };
        let fs = Ext4::load(Box::new(reader))
            .map_err(|err| anyhow::anyhow!("ext4 load failed: {err}"))?;
        let block_size = fs.block_size() as u64;
        Ok(Self {
            image,
            fs,
            source,
            root_parent_id,
            base_offset,
            length_bytes,
            block_size,
        })
    }

    pub fn base_offset(&self) -> u64 {
        self.base_offset
    }

    /// Journal inode number from the superblock's `s_journal_inum` (+0xE0).
    /// Returns `None` when the filesystem has no internal journal (external
    /// journal device, or `^has_journal`). ext4-view does not surface this
    /// field, so read it straight from the superblock at `base_offset + 1024`.
    pub fn journal_inode(&self) -> anyhow::Result<Option<u64>> {
        let sb = self.image.read_range(self.base_offset + 1024, 0x100)?;
        if sb.len() < 0xE0 + 4 {
            anyhow::bail!("ext4 superblock truncated");
        }
        let inum = u32::from_le_bytes([sb[0xE0], sb[0xE1], sb[0xE2], sb[0xE3]]);
        Ok((inum != 0).then_some(inum as u64))
    }

    /// Unallocated bytes in this volume, from the ext4 superblock's free-block
    /// count. ext4-view does not surface `s_free_blocks_count`, so read it
    /// straight from the superblock at `base_offset + 1024`: the low 32 bits at
    /// +12 and the high 32 bits at +0x158 (64-bit feature; zero otherwise),
    /// combined and scaled by the block size. Exact and cheap (one 4 KiB read).
    pub fn free_bytes(&self) -> anyhow::Result<u64> {
        let sb = self.image.read_range(self.base_offset + 1024, 512)?;
        if sb.len() < 0x158 + 4 {
            anyhow::bail!("ext4 superblock truncated");
        }
        let lo = u32::from_le_bytes([sb[12], sb[13], sb[14], sb[15]]) as u64;
        let hi = u32::from_le_bytes([sb[0x158], sb[0x159], sb[0x15a], sb[0x15b]]) as u64;
        let free_blocks = (hi << 32) | lo;
        Ok(free_blocks.saturating_mul(self.block_size))
    }

    pub fn children(
        &self,
        inode_number: u64,
        recover_deleted: bool,
    ) -> anyhow::Result<Vec<FsNode>> {
        let parent_inode = self.read_inode(inode_number)?;
        if parent_inode.kind() != NodeKind::Dir {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        let mut seen: HashSet<(u64, String)> = HashSet::new();

        let inode_idx = inode_number_to_u32(inode_number)?;
        let read_dir = self
            .fs
            .read_dir_inode(inode_idx)
            .map_err(|err| anyhow::anyhow!("read_dir inode {inode_number}: {err}"))?;
        for entry in read_dir {
            let entry =
                entry.map_err(|err| anyhow::anyhow!("dir entry inode {inode_number}: {err}"))?;
            let raw_name = entry.file_name();
            let name = match raw_name.as_str() {
                Ok(name) => name.to_string(),
                Err(_) => String::from_utf8_lossy(raw_name.as_ref()).to_string(),
            };
            if name == "." || name == ".." {
                continue;
            }
            let child_inode = entry.inode_index() as u64;
            if !seen.insert((child_inode, name.clone())) {
                continue;
            }
            let file_type = file_type_byte(
                entry
                    .file_type()
                    .map_err(|err| anyhow::anyhow!("file_type inode {child_inode}: {err}"))?,
            );
            if let Ok(child_meta) = self.read_inode(child_inode) {
                out.push(self.node_from_inode(
                    inode_number,
                    child_inode,
                    &name,
                    file_type,
                    &child_meta,
                ));
            }
        }

        if recover_deleted {
            self.append_recovered_entries(inode_number, &parent_inode, &mut seen, &mut out)?;
        }

        out.sort_by(|a, b| {
            let ak = if a.kind == NodeKind::Dir { 0 } else { 1 };
            let bk = if b.kind == NodeKind::Dir { 0 } else { 1 };
            ak.cmp(&bk)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    }

    fn append_recovered_entries(
        &self,
        parent_inode: u64,
        parent: &Ext4Inode,
        seen: &mut HashSet<(u64, String)>,
        out: &mut Vec<FsNode>,
    ) -> anyhow::Result<()> {
        let runs = data_runs_from_inode(parent, self.base_offset, self.block_size, |offset| {
            self.read_partition_range(offset, self.block_size as usize)
        })?;
        let block_size = self.block_size as usize;
        for run in runs {
            let mut consumed = 0u64;
            while consumed < run.length {
                let chunk_len = (run.length - consumed).min(self.block_size);
                let absolute_offset = run.physical_offset + consumed;
                let block = self.image.read_range(absolute_offset, chunk_len as usize)?;
                if block.is_empty() {
                    break;
                }
                for slack in recover_slack_entries(&block, block_size, seen) {
                    self.push_slack_entry(parent_inode, slack, out);
                }
                consumed += chunk_len;
            }
        }
        Ok(())
    }

    fn push_slack_entry(&self, parent_inode: u64, slack: SlackEntry, out: &mut Vec<FsNode>) {
        let Ok(child_meta) = self.read_inode(slack.child_inode) else {
            return;
        };
        if !matches!(
            child_meta.kind(),
            NodeKind::File | NodeKind::Symlink | NodeKind::Dir
        ) {
            return;
        }
        let mut node = self.node_from_inode(
            parent_inode,
            slack.child_inode,
            &slack.name,
            slack.file_type,
            &child_meta,
        );
        node.id = format!(
            "recover:{}:{parent_inode}:{}",
            self.node_id(slack.child_inode),
            slack.slot_offset
        );
        node.deleted = true;
        out.push(node);
    }

    pub fn node_from_inode(
        &self,
        parent_inode: u64,
        inode_number: u64,
        name: &str,
        file_type: u8,
        inode: &Ext4Inode,
    ) -> FsNode {
        let kind = match file_type {
            1 => NodeKind::File,
            2 => NodeKind::Dir,
            7 => NodeKind::Symlink,
            _ => inode.kind(),
        };
        FsNode {
            id: self.node_id(inode_number),
            name: name.to_string(),
            parent_id: Some(if parent_inode == 2 {
                self.root_parent_id.clone()
            } else {
                self.node_id(parent_inode)
            }),
            kind,
            size: inode.size,
            deleted: inode.dtime != 0,
            inode: Some(inode.number),
            timestamps: inode.timestamps(),
            // 按展示的 kind（目录项 file_type）门控，而不是只看 inode mode：
            // 恢复出的已删除目录项的 inode 可能已被复用成符号链接，把复用后
            // inode 的内容标到旧名字的「文件」行上是无依据的断言。
            link_target: if kind == NodeKind::Symlink {
                self.symlink_target(inode)
            } else {
                None
            },
            has_snapshot: false,
        }
    }

    /// 读符号链接指向的目标路径，按链接内容原样返回（不解析、不跟随）。
    /// 快速符号链接的目标内联在 inode 的 i_block 60 字节里——判定沿用内核
    /// `ext4_inode_is_fast_symlink`：扣除扩展属性块后 `i_blocks` 为 0；
    /// 慢速符号链接的目标在数据块里，走既有的 extent/直接块映射读取。
    /// 任何取不到或不可信（加密、内联超长、疑似损坏）的情况都返回 `None`，
    /// 列表里只是不显示指向——宁缺勿错，绝不把噪声当目标展示。
    pub fn symlink_target(&self, inode: &Ext4Inode) -> Option<String> {
        const EXT4_ENCRYPT_FL: u32 = 0x0000_0800;
        const EXT4_INLINE_DATA_FL: u32 = 0x1000_0000;

        if inode.kind() != NodeKind::Symlink {
            return None;
        }
        // fscrypt 符号链接的 i_block/数据块里是密文（带 2 字节长度头），
        // 不是路径——展示出来就是把乱码当证据。
        if inode.flags & EXT4_ENCRYPT_FL != 0 {
            return None;
        }
        let len = inode.size as usize;
        // 内核 ext4_symlink 拒绝超过一个块的目标，超出即坏数据
        if len == 0 || len as u64 > self.block_size || len > 4096 {
            return None;
        }
        let ea_blocks = if inode.file_acl != 0 {
            // 内核用簇大小（bigalloc），这里用块大小近似；误判走慢速路径
            // 时下方的非 extent 守卫会兜住，不会读出垃圾。
            (self.block_size >> 9) as u32
        } else {
            0
        };
        let is_fast = inode.blocks_lo.saturating_sub(ea_blocks) == 0;
        let bytes = if len <= 60 && is_fast {
            inode.block[..len].to_vec()
        } else if inode.flags & EXT4_INLINE_DATA_FL != 0 {
            // 内联数据符号链接：≤60 字节时与 i_block 字节一致；更长的部分
            // 存在 inode 内 xattr 区，这里没有解析它的机器——宁可不显示。
            if len <= 60 {
                inode.block[..len].to_vec()
            } else {
                return None;
            }
        } else {
            // 慢速符号链接。无 extent 标志且长度本可内联（≤60）的 inode
            // 不走直接块回退：那会把 i_block 里的路径字节/簇账目当块指针
            // 读出垃圾（bigalloc 误判快速为慢速就是这种形态）。
            if len <= 60 && !inode.uses_extents() {
                return None;
            }
            let runs = self.data_runs(inode).ok()?;
            let run = runs.first()?;
            if run.logical_offset != 0 || (run.length as usize) < len {
                return None; // 首块缺失或不完整，目标必然残缺
            }
            self.image.read_range(run.physical_offset, len).ok()?
        };
        let target = String::from_utf8_lossy(&bytes);
        let trimmed = target.trim_end_matches('\0');
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    pub fn read_inode(&self, inode_number: u64) -> anyhow::Result<Ext4Inode> {
        if inode_number == 0 {
            anyhow::bail!("invalid ext4 inode 0");
        }
        let idx = inode_number_to_u32(inode_number)?;
        let bytes = self
            .fs
            .read_raw_inode(idx)
            .map_err(|err| anyhow::anyhow!("read_raw_inode {inode_number}: {err}"))?;
        RawInode::parse(inode_number, &bytes)
    }

    /// Decode the inode-resident and external-block extended attributes for
    /// one inode. Unknown EA-inode values are skipped rather than guessed.
    pub fn extended_attributes(&self, inode_number: u64) -> anyhow::Result<Vec<Ext4Xattr>> {
        let idx = inode_number_to_u32(inode_number)?;
        let bytes = self
            .fs
            .read_raw_inode(idx)
            .map_err(|err| anyhow::anyhow!("read_raw_inode {inode_number}: {err}"))?;
        let inode = RawInode::parse(inode_number, &bytes)?;
        let mut attributes = parse_inode_xattrs(&bytes, inode.extra_isize);
        let block_number = inode.xattr_block();
        if block_number != 0 {
            let offset = block_number.checked_mul(self.block_size).ok_or_else(|| {
                anyhow::anyhow!("ext4 xattr block offset overflow: {block_number}")
            })?;
            let block = self.read_partition_range(offset, self.block_size as usize)?;
            attributes.extend(parse_xattr_block(&block));
        }
        Ok(attributes)
    }

    pub fn read_inode_data(&self, inode: &Ext4Inode, max_bytes: usize) -> anyhow::Result<Vec<u8>> {
        self.read_inode_range(inode, 0, max_bytes)
    }

    /// 随机读 inode 内容的 `[offset, offset + length)`（EOF 截断）。extent 文件按
    /// [`plan_data_pieces`] 只读区间覆盖到的片——与 [`Self::stream_inode_data`] 的字节流
    /// 逐字节一致，代价 O(length) 而不是流式跳读的 O(offset)。
    ///
    /// 为什么不用 `open_inode_by_index` + `read_bytes`：vendored ext4-view 的
    /// `read_bytes` 每次只读一个块，大文件预览（上限 128 MiB）会退化成 3 万次
    /// 包装源随机读；按片读一次一片。
    pub fn read_inode_range(
        &self,
        inode: &Ext4Inode,
        offset: u64,
        length: usize,
    ) -> anyhow::Result<Vec<u8>> {
        let requested_end = offset.saturating_add(u64::try_from(length)?);
        let end = inode.size.min(requested_end);
        if offset >= end {
            return Ok(Vec::new());
        }
        if inode.is_inline_data() {
            // 内联数据：内容直接存在 i_block 里（≤60 字节），与流式读同一片。
            let data = &inode.block[..usize::try_from(inode.size)?.min(inode.block.len())];
            let start = usize::try_from(offset)?.min(data.len());
            let stop = usize::try_from(end)?.min(data.len());
            return Ok(data[start..stop].to_vec());
        }
        if !inode.uses_extents() {
            // 旧式间接块文件（ext4 上稀有）：走 ext4-view 的块级读，正确性优先。
            let prefix = self.read_inode_data_blockwise(inode, usize::try_from(end)?)?;
            let start = usize::try_from(offset)?.min(prefix.len());
            return Ok(prefix[start..].to_vec());
        }
        // 空洞片与镜像外的字节保持 vec![0] 的零值，与流式读补零一致。
        let mut out = vec![0u8; usize::try_from(end - offset)?];
        let runs = self.data_runs(inode)?;
        let mut logical = 0u64;
        for piece in plan_data_pieces(&runs, inode.size, self.image.len()) {
            let piece_end = logical + piece.len();
            if let DataPiece::Data { physical, .. } = piece {
                let from = logical.max(offset);
                let to = piece_end.min(end);
                if from < to {
                    let dst =
                        &mut out[usize::try_from(from - offset)?..usize::try_from(to - offset)?];
                    // 短读（片已按镜像长度裁过，只剩异常来源）的余下字节保持零值，
                    // 与流式读对短读补零一致。
                    self.image.read_into(physical + (from - logical), dst)?;
                }
            }
            logical = piece_end;
            if logical >= end {
                break;
            }
        }
        Ok(out)
    }

    /// 旧式间接块 inode 的块级读（ext4-view 逐块循环）。仅非 extent 文件走
    /// 这里，行为与改写前的实现一致。
    fn read_inode_data_blockwise(&self, inode: &Ext4Inode, len: usize) -> anyhow::Result<Vec<u8>> {
        let idx = inode_number_to_u32(inode.number)?;
        let mut file = self
            .fs
            .open_inode_by_index(idx)
            .map_err(|err| anyhow::anyhow!("open inode {}: {err}", inode.number))?;
        let mut out = vec![0u8; len];
        let mut filled = 0usize;
        while filled < len {
            let read = file
                .read_bytes(&mut out[filled..])
                .map_err(|err| anyhow::anyhow!("read inode {}: {err}", inode.number))?;
            if read == 0 {
                break;
            }
            filled += read;
        }
        out.truncate(filled);
        Ok(out)
    }

    pub fn data_runs(&self, inode: &Ext4Inode) -> anyhow::Result<Vec<FsDataRun>> {
        data_runs_from_inode(inode, self.base_offset, self.block_size, |offset| {
            self.read_partition_range(offset, self.block_size as usize)
        })
    }

    /// 按数据段（`data_runs`）流式读 inode 内容，每段一次底层大读；空洞段补
    /// 零（与 ext4-view 对 hole 的读零语义一致，字节流不变）。
    ///
    /// 为什么不用 `open_inode_by_index` + `File::read_bytes`：vendored
    /// ext4-view 的 `read_bytes` 每次只读一个块（剩余块内字节），1 MiB 请求被
    /// 切成 256 次块读，每次都是一次包装源（VMDK/E01）随机读。包装源的固定
    /// 开销按次计费，这正是搜索全盘时每文件几百 KB 的小文件密集流成为瓶颈的
    /// 根因。按 extent 段读把次数降到「每段一次」。
    ///
    /// 每次调用只分配一块 `min(chunk_size, 文件大小)` 的缓冲，数据片与空洞片共用：
    /// 全盘扫描几十万个小文件时，逐文件的 4 MiB 零缓冲分配/释放本身就是可观开销。
    pub fn stream_inode_data<F>(
        &self,
        inode: &Ext4Inode,
        chunk_size: usize,
        mut visitor: F,
    ) -> anyhow::Result<u64>
    where
        F: FnMut(&[u8]) -> anyhow::Result<()>,
    {
        if inode.size == 0 {
            return Ok(0);
        }
        if inode.is_inline_data() {
            // 内联数据：内容直接存在 i_block 里（≤60 字节），一次发出。
            let data = &inode.block[..(inode.size as usize).min(inode.block.len())];
            visitor(data)?;
            return Ok(data.len() as u64);
        }
        if !inode.uses_extents() {
            // 旧式间接块文件：块级流，行为与改写前一致。
            let idx = inode_number_to_u32(inode.number)?;
            let mut file = self
                .fs
                .open_inode_by_index(idx)
                .map_err(|err| anyhow::anyhow!("open inode {}: {err}", inode.number))?;
            let mut buf = vec![0u8; chunk_size.max(4096)];
            let mut total = 0u64;
            loop {
                let read = file
                    .read_bytes(&mut buf)
                    .map_err(|err| anyhow::anyhow!("stream inode {}: {err}", inode.number))?;
                if read == 0 {
                    break;
                }
                visitor(&buf[..read])?;
                total += read as u64;
            }
            return Ok(total);
        }
        let runs = self.data_runs(inode)?;
        let slab = u64::try_from(chunk_size.max(4096))?.min(inode.size);
        let mut buf = vec![0u8; usize::try_from(slab)?];
        // 缓冲当前是否全零：连续空洞片不必重复清零。
        let mut buf_zeroed = true;
        let mut total = 0u64;
        for piece in plan_data_pieces(&runs, inode.size, self.image.len()) {
            let mut remaining = piece.len();
            let mut physical = match piece {
                DataPiece::Data { physical, .. } => Some(physical),
                DataPiece::Zero(_) => None,
            };
            while remaining > 0 {
                let part = usize::try_from(remaining.min(slab))?;
                match physical.as_mut() {
                    Some(offset) => {
                        let read = self.image.read_into(*offset, &mut buf[..part])?;
                        // 片已按镜像长度裁过；异常来源的短读补零，逻辑位置不变。
                        buf[read..part].fill(0);
                        *offset += part as u64;
                        buf_zeroed = false;
                    }
                    None if !buf_zeroed => {
                        buf.fill(0);
                        buf_zeroed = true;
                    }
                    None => {}
                }
                visitor(&buf[..part])?;
                total += part as u64;
                remaining -= part as u64;
            }
        }
        Ok(total)
    }

    fn read_partition_range(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if offset >= self.length_bytes {
            return Ok(Vec::new());
        }
        let len = (self.length_bytes - offset).min(len as u64) as usize;
        self.image.read_range(self.base_offset + offset, len)
    }

    fn node_id(&self, inode: u64) -> String {
        match self.source {
            Ext4Source::Partition(part_id) => format!("ext4:part:{part_id}:{inode}"),
            Ext4Source::Lvm { part_id, lv_index } => {
                format!("ext4:lvm:{part_id}:{lv_index}:{inode}")
            }
        }
    }
}

fn inode_number_to_u32(inode_number: u64) -> anyhow::Result<u32> {
    u32::try_from(inode_number)
        .map_err(|_| anyhow::anyhow!("inode {inode_number} does not fit in u32"))
}

fn file_type_byte(ft: FileType) -> u8 {
    match ft {
        FileType::Regular => 1,
        FileType::Directory => 2,
        FileType::CharacterDevice => 3,
        FileType::BlockDevice => 4,
        FileType::Fifo => 5,
        FileType::Socket => 6,
        FileType::Symlink => 7,
    }
}

/// Adapter that lets `ext4-view` read straight out of the `DiskImage`
/// abstraction. Reads are clipped to `[base_offset, base_offset +
/// length_bytes)` so the library stays inside the partition / LV slice
/// it was opened on.
struct ImageSliceReader {
    image: Arc<DiskImage>,
    base_offset: u64,
    length_bytes: u64,
}

impl Ext4Read for ImageSliceReader {
    fn read(
        &mut self,
        start_byte: u64,
        dst: &mut [u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let read_len = dst.len();
        let end = start_byte
            .checked_add(read_len as u64)
            .ok_or_else(|| boxed_mem_error(start_byte, read_len, self.length_bytes))?;
        if end > self.length_bytes {
            return Err(boxed_mem_error(start_byte, read_len, self.length_bytes));
        }
        let absolute = self.base_offset + start_byte;
        // Read straight into the caller's buffer: every inode, extent-tree and
        // directory-block read of a filesystem walk or scan comes through here.
        let filled = self.image.read_into(absolute, dst).map_err(
            |err| -> Box<dyn std::error::Error + Send + Sync + 'static> {
                Box::new(std::io::Error::other(err.to_string()))
            },
        )?;
        if filled != read_len {
            return Err(boxed_mem_error(start_byte, read_len, self.length_bytes));
        }
        Ok(())
    }
}

fn boxed_mem_error(
    start_byte: u64,
    read_len: usize,
    src_len: u64,
) -> Box<dyn std::error::Error + Send + Sync + 'static> {
    Box::new(std::io::Error::other(format!(
        "ext4 slice read out of range: {read_len} bytes at offset {start_byte} (slice len {src_len})"
    )))
}
