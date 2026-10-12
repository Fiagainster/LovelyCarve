//! Copy-on-write snapshot reconstruction across the store chain.
//!
//! A shadow copy is not self-contained. Windows saves a block into the diff area
//! of the *newest* store the first time the block is overwritten, so the
//! pre-change bytes of an older snapshot may sit in its own store, in any newer
//! store, or still on the live volume. Reconstructing store `i` therefore
//! consults store `i`, then every newer store in creation order, then the
//! volume. The rules below follow the libvshadow "Reading snapshot data"
//! procedure and are checked block-for-block against pyvshadow by the
//! env-gated `tests/oracle_dfvfs.rs`.
//!
//! Each store's diff-area block list is folded, front to back, into a forward
//! map keyed by the original 16 KiB block:
//! - "not used" descriptors are ignored;
//! - a non-overlay descriptor whose original block is the target of an earlier
//!   forwarder takes over that forwarder's original offset (the reverse list),
//!   and the reverse entry is consumed;
//! - a forwarder that points at its own original offset is ignored;
//! - an overlay merges its 512-byte allocation bitmap into the block's overlay;
//!   a non-overlay descriptor replaces the block's base and keeps its overlay;
//! - every forwarder that becomes (or replaces) a base is recorded in the
//!   reverse list by its target block.
//!
//! Reading the requested (active) store walks the chain block by block:
//! - present in the store: the overlay's allocated sub-blocks (active store
//!   only) or else the base. A forwarder reads the *next* store at its target
//!   offset (the volume when there is no next store); any other descriptor reads
//!   the volume at its store offset. An overlay-only entry is invisible to an
//!   older store's read-through;
//! - absent: the next store at the same offset. Past the newest store the
//!   volume, except when the newest store is itself the active one: a block set
//!   in its current bitmap (and in its previous bitmap, when it has one) that is
//!   not a forwarder target was unallocated at snapshot time and reads as zeros.
//!
//! Metadata is validated while it is loaded: a chain block that is out of range,
//! is not a version-1 VSS block of the expected record type, or is reached
//! twice, and a forwarder that carries a store offset, are
//! [`VssError::CorruptStoreMetadata`]. Data reads that would fall outside the
//! volume are I/O errors and are never zero-filled.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

use crate::block::{BlockDescriptor, BLOCK_DESCRIPTOR_LEN};
use crate::catalog::BLOCK_SIZE;
use crate::error::VssError;
use crate::store::{StoreBlockHeader, STORE_BLOCK_HEADER_LEN};
use crate::VssVolume;

/// Record type of a block-descriptor-list (diff-area) block.
const BLOCK_LIST_RECORD_TYPE: u32 = 0x0003;

/// Record type of a store-bitmap block.
const BITMAP_RECORD_TYPE: u32 = 0x0006;

/// The only store block format version in use.
const STORE_BLOCK_VERSION: u32 = 1;

/// [`BLOCK_SIZE`] as a volume offset.
const BLOCK_BYTES: u64 = BLOCK_SIZE as u64;

/// Size of one overlay sub-block, in bytes.
const SUB_BLOCK_SIZE: usize = 512;

/// Number of 512-byte sub-blocks an overlay allocation bitmap addresses.
const SUB_BLOCK_COUNT: u32 = (BLOCK_SIZE / SUB_BLOCK_SIZE) as u32;

/// Upper bound on the metadata blocks walked for one store, so a corrupt chain
/// cannot keep the loader busy indefinitely.
const MAX_STORE_BLOCKS: usize = 1 << 20;

fn block_number(offset: u64) -> u64 {
    offset / BLOCK_BYTES
}

fn bit_is_set(bitmap: &[u8], block: u64) -> bool {
    usize::try_from(block / 8)
        .ok()
        .and_then(|index| bitmap.get(index))
        .is_some_and(|byte| byte & (1u8 << (block % 8)) != 0)
}

/// The descriptors a store holds for one original block: the base saved block
/// (or forwarder) and the overlay patching 512-byte sub-blocks over it. The base
/// is itself the overlay when the store saved only sub-blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForwardEntry {
    base: BlockDescriptor,
    overlay: Option<BlockDescriptor>,
}

impl ForwardEntry {
    fn overlay(&self) -> Option<&BlockDescriptor> {
        if self.base.flags.is_overlay() {
            Some(&self.base)
        } else {
            self.overlay.as_ref()
        }
    }
}

/// Where the bytes of a piece of a block come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PieceSource {
    /// A descriptor's block: the volume at `offset`, or the next store when the
    /// descriptor is a forwarder and a next store exists.
    Descriptor { offset: u64, forwarder: bool },
    /// Not defined by this store.
    Absent,
}

/// The copy-on-write metadata of one store: its folded diff-area descriptors,
/// the forwarder targets, and its current and previous allocation bitmaps.
///
/// Built by [`VssVolume::store_map`]. Immutable once built, so one map can be
/// shared by every snapshot layout whose chain includes the store.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoreMap {
    /// Forward entries sorted by original block number.
    forward: Vec<(u64, ForwardEntry)>,
    /// Forwarder target block numbers, sorted.
    reverse: Vec<u64>,
    /// Current bitmap; bit `n` set ⇒ block `n` is not in use by the store.
    current_bitmap: Vec<u8>,
    /// Previous bitmap, when the store records one.
    previous_bitmap: Option<Vec<u8>>,
}

impl StoreMap {
    /// Fold a store's block list, in on-disk order, into a map.
    pub(crate) fn build(
        descriptors: &[BlockDescriptor],
        current_bitmap: Vec<u8>,
        previous_bitmap: Option<Vec<u8>>,
    ) -> Self {
        let mut forward: HashMap<u64, ForwardEntry> = HashMap::new();
        // Forwarder target block -> the forwarder's original offset.
        let mut reverse: HashMap<u64, u64> = HashMap::new();

        for raw in descriptors {
            if raw.flags.is_not_used() {
                continue;
            }
            let mut descriptor = *raw;
            if !descriptor.flags.is_overlay() {
                if let Some(original) = reverse.remove(&block_number(descriptor.original_offset)) {
                    descriptor.original_offset = original;
                }
            }
            if descriptor.flags.is_forwarder()
                && descriptor.original_offset == descriptor.relative_store_offset
            {
                continue;
            }
            match forward.entry(block_number(descriptor.original_offset)) {
                Entry::Vacant(slot) => {
                    slot.insert(ForwardEntry {
                        base: descriptor,
                        overlay: None,
                    });
                }
                Entry::Occupied(mut slot) => {
                    let entry = slot.get_mut();
                    if descriptor.flags.is_overlay() {
                        if entry.base.flags.is_overlay() {
                            entry.base.allocation_bitmap |= descriptor.allocation_bitmap;
                        } else if let Some(overlay) = entry.overlay.as_mut() {
                            overlay.allocation_bitmap |= descriptor.allocation_bitmap;
                        } else {
                            entry.overlay = Some(descriptor);
                        }
                        continue;
                    }
                    let replaced = std::mem::replace(&mut entry.base, descriptor);
                    if replaced.flags.is_overlay() {
                        entry.overlay = Some(replaced);
                    }
                }
            }
            if descriptor.flags.is_forwarder() {
                reverse.insert(
                    block_number(descriptor.relative_store_offset),
                    descriptor.original_offset,
                );
            }
        }

        let mut forward: Vec<(u64, ForwardEntry)> = forward.into_iter().collect();
        forward.sort_unstable_by_key(|(block, _)| *block);
        let mut reverse: Vec<u64> = reverse.into_keys().collect();
        reverse.sort_unstable();
        StoreMap {
            forward,
            reverse,
            current_bitmap,
            previous_bitmap,
        }
    }

    /// The number of original blocks the store defines.
    #[must_use]
    pub fn block_count(&self) -> usize {
        self.forward.len()
    }

    fn entry(&self, offset: u64) -> Option<&ForwardEntry> {
        let block = block_number(offset);
        self.forward
            .binary_search_by_key(&block, |(key, _)| *key)
            .ok()
            .and_then(|index| self.forward.get(index))
            .map(|(_, entry)| entry)
    }

    fn is_forwarder_target(&self, offset: u64) -> bool {
        self.reverse.binary_search(&block_number(offset)).is_ok()
    }

    /// Whether the block was unallocated when the snapshot was taken; only
    /// meaningful for the newest store when it is the one being read.
    fn reads_as_zeros(&self, offset: u64) -> bool {
        let block = block_number(offset);
        !self.is_forwarder_target(offset)
            && bit_is_set(&self.current_bitmap, block)
            && self
                .previous_bitmap
                .as_ref()
                .is_none_or(|bitmap| bit_is_set(bitmap, block))
    }
}

/// The owned, reader-independent reconstruction state of one shadow copy: the
/// requested store's [`StoreMap`] followed by every newer store's, in creation
/// order.
///
/// Cheap to clone and `Send + Sync`. Reads take the live-volume reader as a
/// parameter, so one layout can serve any number of positioned readers.
#[derive(Debug, Clone)]
pub struct SnapshotLayout {
    volume_size: u64,
    stores: Vec<Arc<StoreMap>>,
}

impl SnapshotLayout {
    /// Compose a layout from store maps: the requested store first, then each
    /// newer store in creation order (the order of [`VssVolume::stores`]).
    /// `volume_size` is the snapshot's volume size from its catalog entry.
    #[must_use]
    pub fn new(volume_size: u64, stores: Vec<Arc<StoreMap>>) -> Self {
        SnapshotLayout {
            volume_size,
            stores,
        }
    }

    /// The size of the reconstructed volume, in bytes.
    #[must_use]
    pub fn volume_size(&self) -> u64 {
        self.volume_size
    }

    /// Fill `buf` with the snapshot's bytes at `offset`.
    ///
    /// `read_volume(offset, buf)` must fill `buf` exactly from the live volume
    /// (offset 0 = the NTFS boot sector) or fail.
    ///
    /// # Errors
    /// [`io::ErrorKind::UnexpectedEof`] when the range runs past
    /// [`SnapshotLayout::volume_size`], [`io::ErrorKind::InvalidData`] when a
    /// descriptor points outside the addressable range, and any error of
    /// `read_volume`.
    pub fn read_at<F>(&self, offset: u64, buf: &mut [u8], read_volume: &mut F) -> io::Result<()>
    where
        F: FnMut(u64, &mut [u8]) -> io::Result<()>,
    {
        let in_range = u64::try_from(buf.len())
            .ok()
            .and_then(|len| offset.checked_add(len))
            .is_some_and(|end| end <= self.volume_size);
        if !in_range {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "shadow copy read of {} bytes at {offset:#x} runs past the {}-byte volume",
                    buf.len(),
                    self.volume_size
                ),
            ));
        }
        self.read_chain(0, offset, buf, read_volume)
    }

    fn read_chain<F>(
        &self,
        depth: usize,
        mut offset: u64,
        buf: &mut [u8],
        read_volume: &mut F,
    ) -> io::Result<()>
    where
        F: FnMut(u64, &mut [u8]) -> io::Result<()>,
    {
        let Some(store) = self.stores.get(depth) else {
            return read_volume(offset, buf);
        };
        let has_next = depth + 1 < self.stores.len();
        let is_active = depth == 0;

        let mut done = 0usize;
        while done < buf.len() {
            let within = (offset % BLOCK_BYTES) as usize;
            let (source, piece) = piece_at(store, offset, within, is_active);
            let length = piece.min(buf.len() - done);
            let out = &mut buf[done..done + length];
            match source {
                PieceSource::Descriptor {
                    offset: block,
                    forwarder,
                } => {
                    let target = block.checked_add(within as u64).ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("shadow copy descriptor offset {block:#x} overflows"),
                        )
                    })?;
                    if forwarder && has_next {
                        self.read_chain(depth + 1, target, out, read_volume)?;
                    } else {
                        read_volume(target, out)?;
                    }
                }
                PieceSource::Absent if has_next => {
                    self.read_chain(depth + 1, offset, out, read_volume)?;
                }
                PieceSource::Absent if is_active && store.reads_as_zeros(offset) => out.fill(0),
                PieceSource::Absent => read_volume(offset, out)?,
            }
            done += length;
            offset += length as u64;
        }
        Ok(())
    }
}

/// Resolve the source of the bytes at `offset` (`within` bytes into its block)
/// and how many bytes from there share it.
fn piece_at(store: &StoreMap, offset: u64, within: usize, is_active: bool) -> (PieceSource, usize) {
    let to_block_end = BLOCK_SIZE - within;
    let Some(entry) = store.entry(offset) else {
        return (PieceSource::Absent, to_block_end);
    };
    let base = &entry.base;
    let base_source = PieceSource::Descriptor {
        offset: if base.flags.is_forwarder() {
            base.relative_store_offset
        } else {
            base.store_offset
        },
        forwarder: base.flags.is_forwarder(),
    };
    let overlay_only = base.flags.is_overlay();
    let Some(overlay) = entry.overlay() else {
        return (base_source, to_block_end);
    };
    if !is_active {
        // Overlays apply to the requested store only.
        let source = if overlay_only {
            PieceSource::Absent
        } else {
            base_source
        };
        return (source, to_block_end);
    }

    let sub_block = (within / SUB_BLOCK_SIZE) as u32;
    let shifted = overlay.allocation_bitmap >> sub_block;
    let covered = shifted & 1 != 0;
    let run = if covered {
        shifted.trailing_ones()
    } else {
        (!shifted).trailing_ones()
    }
    .min(SUB_BLOCK_COUNT - sub_block) as usize;
    let piece = run * SUB_BLOCK_SIZE - within % SUB_BLOCK_SIZE;
    let source = if covered {
        PieceSource::Descriptor {
            offset: overlay.store_offset,
            forwarder: overlay.flags.is_forwarder(),
        }
    } else if overlay_only {
        PieceSource::Absent
    } else {
        base_source
    };
    (source, piece)
}

/// A reconstructed, read-only view of one shadow copy over the volume's reader.
///
/// Built by [`VssVolume::snapshot`]; borrows the volume's reader mutably so
/// blocks are materialized on demand (the volumes are far too large to hold in
/// memory). For an owned handle use [`Snapshot::layout`] /
/// [`VssVolume::snapshot_layout`] with a reader of your own.
pub struct Snapshot<'v, R> {
    reader: &'v mut R,
    layout: SnapshotLayout,
}

impl<R: Read + Seek> VssVolume<R> {
    /// Load and fold the copy-on-write metadata of store `index`.
    ///
    /// # Errors
    /// - [`VssError::StoreIndexOutOfRange`] if `index` is past the last store.
    /// - [`VssError::StoreInfoUnavailable`] if the store has no type-0x03
    ///   catalog pointer, so its diff area cannot be located.
    /// - [`VssError::CorruptStoreMetadata`] for an invalid diff-area or bitmap
    ///   chain.
    /// - [`VssError::Io`] on an underlying read/seek failure.
    pub fn store_map(&mut self, index: usize) -> Result<StoreMap, VssError> {
        let count = self.stores.len();
        let store = self
            .stores
            .get(index)
            .ok_or(VssError::StoreIndexOutOfRange { index, count })?;
        let (Some(block_list_offset), Some(bitmap_offset)) =
            (store.store_block_list_offset, store.store_bitmap_offset)
        else {
            return Err(VssError::StoreInfoUnavailable { index });
        };
        let previous_bitmap_offset = store.store_previous_bitmap_offset;

        let mut chains = ChainReader {
            reader: &mut self.reader,
            volume_size: self.volume_size,
            store: index,
            visited: HashSet::new(),
            block: vec![0u8; BLOCK_SIZE],
        };
        let current_bitmap = chains.read_bitmap(bitmap_offset)?;
        let previous_bitmap = previous_bitmap_offset
            .map(|offset| chains.read_bitmap(offset))
            .transpose()?;
        let descriptors = chains.read_block_list(block_list_offset)?;
        Ok(StoreMap::build(
            &descriptors,
            current_bitmap,
            previous_bitmap,
        ))
    }

    /// Build the reconstruction layout of store `index`: its own metadata and
    /// that of every newer store.
    ///
    /// # Errors
    /// As [`VssVolume::store_map`], for store `index` or any newer store.
    pub fn snapshot_layout(&mut self, index: usize) -> Result<SnapshotLayout, VssError> {
        let count = self.stores.len();
        let volume_size = self
            .stores
            .get(index)
            .ok_or(VssError::StoreIndexOutOfRange { index, count })?
            .volume_size;
        let mut maps = Vec::with_capacity(count - index);
        for chain_index in index..count {
            maps.push(Arc::new(self.store_map(chain_index)?));
        }
        Ok(SnapshotLayout::new(volume_size, maps))
    }

    /// Reconstruct store `index` over this volume's reader.
    ///
    /// # Errors
    /// As [`VssVolume::snapshot_layout`].
    pub fn snapshot(&mut self, index: usize) -> Result<Snapshot<'_, R>, VssError> {
        let layout = self.snapshot_layout(index)?;
        Ok(Snapshot {
            reader: &mut self.reader,
            layout,
        })
    }
}

impl<R: Read + Seek> Snapshot<'_, R> {
    /// The owned reconstruction state behind this view.
    #[must_use]
    pub fn layout(&self) -> &SnapshotLayout {
        &self.layout
    }

    /// Reconstruct the aligned 16384-byte block containing `offset`.
    ///
    /// # Errors
    /// As [`Snapshot::read_at`].
    pub fn read_block(&mut self, offset: u64) -> Result<[u8; BLOCK_SIZE], VssError> {
        let mut block = [0u8; BLOCK_SIZE];
        self.read_at(offset - offset % BLOCK_BYTES, &mut block)?;
        Ok(block)
    }

    /// Fill `buf` with the snapshot's bytes at `offset`.
    ///
    /// # Errors
    /// [`VssError::Io`] when the range runs past the volume, a descriptor
    /// points outside it, or the underlying read/seek fails.
    pub fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), VssError> {
        let reader = &mut *self.reader;
        self.layout
            .read_at(offset, buf, &mut |volume_offset, out: &mut [u8]| {
                reader.seek(SeekFrom::Start(volume_offset))?;
                reader.read_exact(out)
            })?;
        Ok(())
    }
}

/// Walks one store's metadata chains, rejecting any block read twice across
/// them (as a cycle or cross-linked chain would).
struct ChainReader<'a, R> {
    reader: &'a mut R,
    volume_size: u64,
    store: usize,
    visited: HashSet<u64>,
    block: Vec<u8>,
}

impl<R: Read + Seek> ChainReader<'_, R> {
    fn corrupt(&self, offset: u64, reason: &'static str) -> VssError {
        VssError::CorruptStoreMetadata {
            index: self.store,
            offset,
            reason,
        }
    }

    /// Visit the payload of every block of the chain starting at `first`.
    fn walk(
        &mut self,
        first: u64,
        record_type: u32,
        mut visit: impl FnMut(&[u8]) -> Result<(), &'static str>,
    ) -> Result<(), VssError> {
        let mut next = first;
        let mut blocks = 0usize;
        while next != 0 {
            if blocks == MAX_STORE_BLOCKS {
                return Err(self.corrupt(next, "metadata chain exceeds the block limit"));
            }
            if !self.visited.insert(next) {
                return Err(self.corrupt(next, "metadata block is reached twice"));
            }
            if next
                .checked_add(BLOCK_BYTES)
                .is_none_or(|end| end > self.volume_size)
            {
                return Err(self.corrupt(next, "metadata block runs past the volume"));
            }
            self.reader.seek(SeekFrom::Start(next))?;
            self.reader.read_exact(&mut self.block)?;
            let header = StoreBlockHeader::parse(&self.block);
            if !header.has_vss_identifier || header.version != STORE_BLOCK_VERSION {
                return Err(self.corrupt(next, "not a version-1 VSS store block"));
            }
            if header.record_type != record_type {
                return Err(self.corrupt(next, "unexpected store block record type"));
            }
            if let Err(reason) = visit(&self.block[STORE_BLOCK_HEADER_LEN..]) {
                return Err(self.corrupt(next, reason));
            }
            blocks += 1;
            next = header.next_offset;
        }
        Ok(())
    }

    /// Concatenate a bitmap chain's payloads in chain order.
    fn read_bitmap(&mut self, first: u64) -> Result<Vec<u8>, VssError> {
        let mut bitmap = Vec::new();
        self.walk(first, BITMAP_RECORD_TYPE, |payload| {
            bitmap.extend_from_slice(payload);
            Ok(())
        })?;
        Ok(bitmap)
    }

    /// Collect a block list's descriptors in on-disk order, skipping empty slots.
    fn read_block_list(&mut self, first: u64) -> Result<Vec<BlockDescriptor>, VssError> {
        let mut descriptors = Vec::new();
        self.walk(first, BLOCK_LIST_RECORD_TYPE, |payload| {
            let (records, _) = payload.as_chunks::<BLOCK_DESCRIPTOR_LEN>();
            for record in records {
                if record.iter().all(|&byte| byte == 0) {
                    continue;
                }
                let descriptor = BlockDescriptor::parse(record);
                if descriptor.flags.is_forwarder() && descriptor.store_offset != 0 {
                    return Err("forwarder descriptor carries a store offset");
                }
                descriptors.push(descriptor);
            }
            Ok(())
        })?;
        Ok(descriptors)
    }
}
