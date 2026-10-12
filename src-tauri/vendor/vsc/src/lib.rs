//! # vsc-core — Windows Volume Shadow Copy (VSS) reader
//!
//! A panic-free decoder for the on-disk structures of Windows Volume Shadow Copy
//! (VSS), the `[P^H]` disk-history substrate of the forensic fleet. Given a
//! positioned `Read + Seek` over an NTFS **volume** (offset 0 = the NTFS boot
//! sector), [`VssVolume::open`] reads the VSS volume header at byte offset
//! `0x1E00`, walks the catalog of shadow-copy stores, and exposes each store's
//! [`StoreInfo`] on demand.
//!
//! The reader stays pure: it decodes bytes into typed records and makes no
//! forensic judgments (those live in the sibling `vsc-forensic` analyzer). It
//! never loads the whole volume into memory — the real volumes are hundreds of
//! gigabytes — and every multi-byte read is bounds-checked, so malformed input
//! yields safe defaults or a typed [`VssError`], never a panic.
//!
//! It enumerates stores and decodes store information plus the typed diff-area
//! records ([`BlockDescriptor`], [`StoreBlockRange`]), and reconstructs a
//! snapshot's copy-on-write view of the volume via [`VssVolume::snapshot`] →
//! [`Snapshot::read_block`] / [`Snapshot::read_at`], or through the owned,
//! reader-independent [`SnapshotLayout`] built from per-store [`StoreMap`]s.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};

pub mod block;
mod bytes;
pub mod catalog;
pub mod error;
pub mod guid;
pub mod reconstruct;
pub mod store;

#[cfg(test)]
mod tests;

pub use block::{BlockDescriptor, BlockDescriptorFlags, StoreBlockRange};
pub use catalog::{StoreDescriptor, VolumeHeader};
pub use error::VssError;
pub use guid::{format_guid, VSS_IDENTIFIER};
pub use reconstruct::{Snapshot, SnapshotLayout, StoreMap};
pub use store::{AttributeFlags, StoreBlockHeader, StoreInfo};

use catalog::{
    catalog_next_block_offset, is_catalog_block, parse_catalog_entry, CatalogEntry, BLOCK_SIZE,
    CATALOG_BLOCK_HEADER_LEN, CATALOG_ENTRY_LEN, MAX_CATALOG_BLOCKS, VSS_VOLUME_HEADER_OFFSET,
};
use store::{MAX_STORE_INFO_LEN, STORE_BLOCK_HEADER_LEN};

/// A read-only view over the Volume Shadow Copy metadata of an NTFS volume.
///
/// Construct with [`VssVolume::open`] over a `Read + Seek` positioned at the
/// start of the NTFS volume. The catalog of stores is read eagerly (it is
/// small) and ordered by creation time, oldest first — the order the
/// copy-on-write chain is applied in; each store's [`StoreInfo`] is read lazily
/// via [`VssVolume::store_info`].
#[derive(Debug)]
pub struct VssVolume<R> {
    reader: R,
    volume_size: u64,
    has_vss_header: bool,
    catalog_offset: u64,
    stores: Vec<StoreDescriptor>,
}

impl<R: Read + Seek> VssVolume<R> {
    /// Open a VSS view over a positioned NTFS volume reader.
    ///
    /// Reads the volume header at `0x1E00`; if it carries the VSS identifier and
    /// names a catalog, walks the catalog and enumerates the stores. A volume
    /// with no VSS header (the header region is zeroed) opens successfully with
    /// [`VssVolume::has_vss_header`] `== false` and zero stores.
    ///
    /// # Errors
    /// Returns [`VssError::Io`] on an underlying read/seek failure.
    pub fn open(mut reader: R) -> Result<Self, VssError> {
        let volume_size = reader.seek(SeekFrom::End(0))?;

        let mut has_vss_header = false;
        let mut catalog_offset = 0u64;
        if volume_size >= VSS_VOLUME_HEADER_OFFSET + CATALOG_BLOCK_HEADER_LEN as u64 {
            reader.seek(SeekFrom::Start(VSS_VOLUME_HEADER_OFFSET))?;
            let mut hdr = [0u8; CATALOG_BLOCK_HEADER_LEN];
            reader.read_exact(&mut hdr)?;
            let vh = VolumeHeader::parse(&hdr);
            has_vss_header = vh.has_vss_identifier;
            catalog_offset = vh.catalog_offset;
        }

        let stores = if has_vss_header && catalog_offset != 0 {
            walk_catalog(&mut reader, catalog_offset, volume_size)?
        } else {
            Vec::new()
        };

        Ok(VssVolume {
            reader,
            volume_size,
            has_vss_header,
            catalog_offset,
            stores,
        })
    }

    /// Whether the volume carries a VSS volume header at `0x1E00`.
    #[must_use]
    pub fn has_vss_header(&self) -> bool {
        self.has_vss_header
    }

    /// The enumerated shadow-copy store descriptors.
    #[must_use]
    pub fn stores(&self) -> &[StoreDescriptor] {
        &self.stores
    }

    /// The number of enumerated shadow-copy stores.
    #[must_use]
    pub fn store_count(&self) -> usize {
        self.stores.len()
    }

    /// The catalog offset from the volume header (0 when there is no catalog).
    #[must_use]
    pub fn catalog_offset(&self) -> u64 {
        self.catalog_offset
    }

    /// The total size of the underlying volume, in bytes.
    #[must_use]
    pub fn volume_size(&self) -> u64 {
        self.volume_size
    }

    /// Read and decode the store information for store `index`.
    ///
    /// # Errors
    /// - [`VssError::StoreIndexOutOfRange`] if `index` is past the last store.
    /// - [`VssError::StoreInfoUnavailable`] if the store has no type-0x03
    ///   catalog pointer.
    /// - [`VssError::StoreOffsetOutOfBounds`] if the store-header offset runs
    ///   past the end of the volume.
    /// - [`VssError::Io`] on an underlying read/seek failure.
    pub fn store_info(&mut self, index: usize) -> Result<StoreInfo, VssError> {
        let count = self.stores.len();
        let header_off = self
            .stores
            .get(index)
            .ok_or(VssError::StoreIndexOutOfRange { index, count })?
            .store_header_offset
            .ok_or(VssError::StoreInfoUnavailable { index })?;

        // Range-check the store-header offset before seeking: it comes straight
        // from the catalog and must not be trusted.
        let header_end = header_off
            .checked_add(STORE_BLOCK_HEADER_LEN as u64)
            .filter(|end| *end <= self.volume_size)
            .ok_or(VssError::StoreOffsetOutOfBounds {
                index,
                offset: header_off,
                volume_size: self.volume_size,
            })?;

        self.reader.seek(SeekFrom::Start(header_off))?;
        let mut hbuf = [0u8; STORE_BLOCK_HEADER_LEN];
        self.reader.read_exact(&mut hbuf)?;
        let header = StoreBlockHeader::parse(&hbuf);

        // Cap the store-information read against a lying size field and the tail
        // of the volume.
        let remaining = self.volume_size.saturating_sub(header_end) as usize;
        let want = usize::try_from(header.store_information_size).unwrap_or(usize::MAX);
        let info_len = want.min(MAX_STORE_INFO_LEN).min(remaining);
        let mut ibuf = vec![0u8; info_len];
        self.reader.read_exact(&mut ibuf)?;
        Ok(StoreInfo::parse(&ibuf))
    }
}

/// Walk the catalog block chain starting at `first`, collecting every snapshot
/// descriptor and attaching each store-header offset from the paired type-0x03
/// entry.
///
/// The walk is bounded three ways against a corrupt or adversarial catalog: a
/// visited-set breaks any offset cycle, [`MAX_CATALOG_BLOCKS`] caps the chain
/// length, and every block is range-checked against the volume before it is
/// read.
fn walk_catalog<R: Read + Seek>(
    reader: &mut R,
    first: u64,
    volume_size: u64,
) -> Result<Vec<StoreDescriptor>, VssError> {
    let mut stores: Vec<StoreDescriptor> = Vec::new();
    let mut visited: HashSet<u64> = HashSet::new();
    let mut next = first;
    let mut blocks = 0usize;

    while next != 0 && blocks < MAX_CATALOG_BLOCKS {
        if !visited.insert(next) {
            break; // cycle
        }
        match next.checked_add(BLOCK_SIZE as u64) {
            Some(end) if end <= volume_size => {}
            _ => break, // out of range / overflow
        }

        reader.seek(SeekFrom::Start(next))?;
        let mut block = vec![0u8; BLOCK_SIZE];
        reader.read_exact(&mut block)?;
        if !is_catalog_block(&block) {
            break;
        }

        let mut off = CATALOG_BLOCK_HEADER_LEN;
        while off + CATALOG_ENTRY_LEN <= BLOCK_SIZE {
            match parse_catalog_entry(&block[off..off + CATALOG_ENTRY_LEN]) {
                CatalogEntry::Snapshot(descriptor) => stores.push(descriptor),
                CatalogEntry::StorePointer {
                    store_id,
                    store_block_list_offset,
                    store_header_offset,
                    store_bitmap_offset,
                    store_previous_bitmap_offset,
                } => {
                    if let Some(descriptor) = stores
                        .iter_mut()
                        .rev()
                        .find(|d| d.store_id == store_id && d.store_header_offset.is_none())
                    {
                        descriptor.store_header_offset = Some(store_header_offset);
                        descriptor.store_bitmap_offset = Some(store_bitmap_offset);
                        descriptor.store_block_list_offset = Some(store_block_list_offset);
                        descriptor.store_previous_bitmap_offset = (store_previous_bitmap_offset
                            != 0)
                            .then_some(store_previous_bitmap_offset);
                    }
                }
                CatalogEntry::Empty | CatalogEntry::Other => {}
            }
            off += CATALOG_ENTRY_LEN;
        }

        blocks += 1;
        next = catalog_next_block_offset(&block);
    }

    // Reconstruction applies the stores newest-first over the live volume, so
    // the chain order is creation time (oldest first), not catalog position.
    // The sort is stable: equal timestamps keep their catalog order.
    stores.sort_by_key(|descriptor| descriptor.creation_time);
    Ok(stores)
}
