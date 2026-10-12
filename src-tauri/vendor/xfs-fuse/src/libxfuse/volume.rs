/*
 * BSD 2-Clause License
 *
 * Copyright (c) 2021, Khaled Emara
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are met:
 *
 * 1. Redistributions of source code must retain the above copyright notice, this
 *    list of conditions and the following disclaimer.
 *
 * 2. Redistributions in binary form must reproduce the above copyright notice,
 *    this list of conditions and the following disclaimer in the documentation
 *    and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
 * AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
 * DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
 * SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
 * CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

//! Read-only XFS volume API.
//!
//! Replaces the upstream FUSE `Filesystem` implementation with a small
//! blocking, panic-tolerant API the lovelycarve dispatcher can drive
//! directly. The on-disk parser still expects the superblock to be reachable
//! from a process-global slot during decoding (a few `Decode` impls peek at
//! `sb.has_ftype()` / `sb.sb_uuid` while running through bincode), so we keep
//! the `OnceLock` pattern but expose it as a "set on first volume open"
//! single-shot — every XFS partition we open in the same image shares the
//! same superblock geometry in practice (`mkfs.xfs` defaults), so a single
//! published `Sb` is enough.

use std::cell::Cell;
use std::io::{Read, Seek};
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::block_reader::BlockReader;
use super::compat::{FileAttr, FileType, EIO, ENOENT};
use super::definitions::XfsIno;
use super::dinode::Dinode;
use super::dir3::Dir3;
use super::sb::Sb;

thread_local! {
    static CURRENT_SUPERBLOCK: Cell<Option<Sb>> = const { Cell::new(None) };
}

/// Thread-local superblock slot. The XFS bincode `Decode` impls reach in here
/// mid-decode (e.g. `Dir2DataEntry::Decode` checks `sb.has_ftype()`,
/// `XfsDa3Blkinfo::Decode` cross-checks `sb.sb_uuid`). Each public `Volume`
/// operation binds its own superblock before decoding, so a process can browse
/// multiple XFS volumes with different UUIDs without the first opened volume
/// poisoning later lookups on the same image.
pub(crate) struct SuperblockSlot;

impl SuperblockSlot {
    pub(crate) fn set(&self, superblock: Sb) {
        CURRENT_SUPERBLOCK.with(|slot| slot.set(Some(superblock)));
    }

    pub(crate) fn get(&self) -> Option<Sb> {
        CURRENT_SUPERBLOCK.with(Cell::get)
    }
}

pub(crate) static SUPERBLOCK: SuperblockSlot = SuperblockSlot;

/// One directory entry surfaced by `Volume::readdir`.
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub ino: XfsIno,
    /// Cookie that callers feed back into `readdir(parent_ino, offset)` to
    /// resume after this entry. Internal to the on-disk format — opaque to
    /// the rest of the system.
    pub offset: i64,
    /// `None` if the on-disk directory was created without ftype recording
    /// (older XFS without `XFS_SB_FEAT_INCOMPAT_FTYPE`); the caller can
    /// recover the real kind via `getattr`.
    pub kind: Option<FileType>,
    /// Raw on-disk filename bytes. POSIX semantics — caller decides how to
    /// interpret as UTF-8 / WTF-8 / Latin-1.
    pub name: Vec<u8>,
}

/// Read-only XFS volume backed by a buffered, block-aligned reader.
pub struct Volume<R: Read + Seek> {
    pub device: BlockReader<R>,
    pub sb: Sb,
    /// Cached root inode, kept only to confirm the volume opened (the root is
    /// re-read on demand). `None` when even the root inode failed to parse —
    /// every per-inode operation then surfaces the error lazily instead of
    /// panicking at open time.
    root: Option<Dinode>,
}

impl<R: Read + Seek> Volume<R> {
    /// Build a `Volume` over the given block device.
    pub fn from(reader: R) -> Self {
        let mut device = BlockReader::new(reader);
        let superblock = Sb::from(&mut device);
        SUPERBLOCK.set(superblock);
        device.set_bufsize(superblock.inode_size());
        let root =
            catch_xfs_panic(|| Dinode::from(&mut device, &superblock, superblock.sb_rootino)).ok();
        Volume {
            device,
            sb: superblock,
            root,
        }
    }

    /// Read the dinode for `ino`, mapping the FUSE-style root sentinel `1`
    /// to `sb.sb_rootino` for caller convenience. `Err(errno)` when the inode
    /// cannot be parsed (a freed/zeroed inode still referenced by a directory
    /// entry, an unsupported inode type/format, or an out-of-range number),
    /// so a whole-tree walk degrades per inode instead of panicking.
    fn read_dinode(&mut self, ino: XfsIno) -> Result<Dinode, i32> {
        SUPERBLOCK.set(self.sb);
        if ino == 1 || ino == self.sb.sb_rootino {
            return self.root_dinode();
        }
        self.device.set_bufsize(self.sb.inode_size());
        catch_xfs_panic(|| Dinode::from(&mut self.device, &self.sb, ino))
    }

    fn root_dinode(&mut self) -> Result<Dinode, i32> {
        // Re-read so the caller gets a fresh handle; the cached `root` is
        // there only to validate that the volume opened successfully.
        SUPERBLOCK.set(self.sb);
        self.device.set_bufsize(self.sb.inode_size());
        catch_xfs_panic(|| Dinode::from(&mut self.device, &self.sb, self.sb.sb_rootino))
    }

    /// Resolve a single name within the directory inode `parent_ino`.
    pub fn lookup(&mut self, parent_ino: XfsIno, name: &[u8]) -> Result<XfsIno, i32> {
        SUPERBLOCK.set(self.sb);
        catch_xfs_panic(|| {
            let mut parent = self.read_dinode(parent_ino)?;
            let dirsize = (self.sb.sb_blocksize as u64) << self.sb.sb_dirblklog;
            self.device.set_bufsize(dirsize as usize);
            let dir = parent.get_dir(&mut self.device, &self.sb);
            dir.lookup(&mut self.device, &self.sb, name)
        })
    }

    /// Walk a `/`-rooted UTF-8 path from the root, returning the leaf inode.
    pub fn lookup_path(&mut self, path: &str) -> Result<XfsIno, i32> {
        let mut current = self.sb.sb_rootino;
        for component in path.split('/').filter(|c| !c.is_empty()) {
            current = self.lookup(current, component.as_bytes())?;
        }
        Ok(current)
    }

    /// Build a `FileAttr` for the given inode.
    pub fn getattr(&mut self, ino: XfsIno) -> Result<FileAttr, i32> {
        SUPERBLOCK.set(self.sb);
        catch_xfs_panic(|| {
            let dinode = self.read_dinode(ino)?;
            dinode.di_core.stat(ino)
        })
    }

    /// List one directory's children. Returns up to `MAX_ENTRIES` entries —
    /// enough for any directory we expect on a forensic image; if a real
    /// volume bumps into the cap the caller should switch to a paginating
    /// API.
    pub fn readdir(&mut self, parent_ino: XfsIno) -> Result<Vec<DirEntry>, i32> {
        SUPERBLOCK.set(self.sb);
        catch_xfs_panic(|| {
            const MAX_ENTRIES: usize = 100_000;
            let mut parent = self.read_dinode(parent_ino)?;
            let dirsize = (self.sb.sb_blocksize as u64) << self.sb.sb_dirblklog;
            self.device.set_bufsize(dirsize as usize);
            let dir = parent.get_dir(&mut self.device, &self.sb);
            let mut out = Vec::new();
            let mut offset: i64 = 0;
            loop {
                match dir.next(&mut self.device, &self.sb, offset) {
                    Ok((ino, next_offset, kind, name)) => {
                        out.push(DirEntry {
                            ino,
                            offset: next_offset,
                            kind,
                            name,
                        });
                        if out.len() >= MAX_ENTRIES {
                            break;
                        }
                        offset = next_offset;
                    }
                    // The upstream code uses ENOENT / -1 as "end of directory"
                    // sentinels; treat both as a clean stop. Other errno values
                    // are real corruption — propagate.
                    Err(ENOENT) | Err(-1) => break,
                    Err(e) => return Err(e),
                }
            }
            Ok(out)
        })
    }

    /// Read up to `size` bytes at `offset` from a regular file. Returns the
    /// owned bytes (possibly less than `size` near EOF).
    pub fn read(&mut self, ino: XfsIno, offset: i64, size: u32) -> Result<Vec<u8>, i32> {
        SUPERBLOCK.set(self.sb);
        catch_xfs_panic(|| {
            let dinode = self.read_dinode(ino)?;
            // This portable reader has no separate real-time device source.
            // Never interpret its extent addresses on the data device.
            if dinode.di_core.is_realtime() {
                return Err(super::compat::EINVAL);
            }
            self.device.set_bufsize(self.sb.sb_blocksize as usize);
            let file = dinode.get_file::<BlockReader<R>>(&mut self.device);
            let (buf, ignore) = file.read(&mut self.device, offset, size)?;
            let trimmed = buf[ignore..].to_vec();
            Ok(trimmed)
        })
    }

    /// Read the target of a symlink inode, returned as raw bytes (no NUL).
    pub fn readlink(&mut self, ino: XfsIno) -> Result<Vec<u8>, i32> {
        SUPERBLOCK.set(self.sb);
        catch_xfs_panic(|| {
            let dinode = self.read_dinode(ino)?;
            self.device.set_bufsize(self.sb.sb_blocksize as usize);
            Ok(dinode.get_link_data(&mut self.device, &self.sb))
        })
    }

    /// Borrow the parsed superblock (for callers that need to render volume
    /// metadata in their own UI).
    pub fn superblock(&self) -> &Sb {
        &self.sb
    }
}

fn catch_xfs_panic<T>(op: impl FnOnce() -> Result<T, i32>) -> Result<T, i32> {
    match catch_unwind(AssertUnwindSafe(op)) {
        Ok(result) => result,
        Err(_) => Err(EIO),
    }
}
