//! Raw exFAT deleted-entry, data-run, and lenient-scan recovery — the forensic
//! depth the vendored `exfat` crate does not provide (it exposes neither raw
//! directory clusters, the FAT, the allocation bitmap, nor cluster geometry).
//!
//! exFAT directory entries come in 32-byte typed slots forming an *entry set*:
//! a File entry (`0x85`) names a `secondary_count`, followed by one Stream
//! Extension (`0xC0`) carrying the data location and N File Name entries
//! (`0xC1`) carrying 15 UTF-16 chars each. Clearing the high bit of the type
//! byte (→ `0x05` / `0x40` / `0x41`) marks the set deleted (TSK
//! `exfatfs_dent.c`). The Stream Extension's `NoFatChain` flag (bit 1) means
//! the file is contiguous: `first_cluster` + `data_length` give a single run
//! with no FAT traversal (`exfatfs_meta.c`).
//!
//! Everything here parses raw bytes from [`DiskImage::read_range`].

use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveDate, TimeZone, Utc};

use crate::core::evidence::image::DiskImage;
use crate::core::filesystem::tree::fs_tree::{FsDataRun, Mactb};

const SLOT_LEN: usize = 32;
const IN_USE_BIT: u8 = 0x80;
const TYPE_FILE: u8 = 0x85;
const TYPE_STREAM: u8 = 0xC0;
const TYPE_NAME: u8 = 0xC1;
const NO_FAT_CHAIN: u8 = 0x02;
const ATTR_DIRECTORY: u16 = 0x10;
const MAX_DIR_CLUSTERS: usize = 1 << 20;
const BOOT_SECTOR_LEN: usize = 512;
/// exFAT Backup Boot Region: sectors 12-23 mirror the Main Boot Region (0-11),
/// so the backup VBR (with the BPB-equivalent fields) sits at sector 12.
const EXFAT_BACKUP_BOOT_SECTOR: u64 = 12;

/// Parsed exFAT geometry from the boot sector (offsets per the exFAT spec).
#[derive(Debug, Clone, Copy)]
pub struct ExfatGeom {
    pub base_offset: u64,
    pub fat_offset_sectors: u64,
    pub fat_length_sectors: u64,
    pub cluster_heap_offset_sectors: u64,
    pub cluster_count: u32,
    pub root_cluster: u32,
    pub bytes_per_sector: u64,
    pub sectors_per_cluster: u64,
}

impl ExfatGeom {
    pub fn parse(boot: &[u8], base_offset: u64) -> Result<Self> {
        if boot.len() < 512 || &boot[3..11] != b"EXFAT   " {
            anyhow::bail!("not an exFAT boot sector");
        }
        let fat_offset_sectors =
            u32::from_le_bytes([boot[80], boot[81], boot[82], boot[83]]) as u64;
        let fat_length_sectors =
            u32::from_le_bytes([boot[84], boot[85], boot[86], boot[87]]) as u64;
        let cluster_heap_offset_sectors =
            u32::from_le_bytes([boot[88], boot[89], boot[90], boot[91]]) as u64;
        let cluster_count = u32::from_le_bytes([boot[92], boot[93], boot[94], boot[95]]);
        let root_cluster = u32::from_le_bytes([boot[96], boot[97], boot[98], boot[99]]);
        let bytes_per_sector_shift = boot[108];
        let sectors_per_cluster_shift = boot[109];
        if bytes_per_sector_shift > 12 || sectors_per_cluster_shift > 25 {
            anyhow::bail!("exFAT geometry shifts out of range");
        }
        let bytes_per_sector = 1u64 << bytes_per_sector_shift;
        let sectors_per_cluster = 1u64 << sectors_per_cluster_shift;
        Ok(Self {
            base_offset,
            fat_offset_sectors,
            fat_length_sectors,
            cluster_heap_offset_sectors,
            cluster_count,
            root_cluster,
            bytes_per_sector,
            sectors_per_cluster,
        })
    }

    fn cluster_size(&self) -> u64 {
        self.bytes_per_sector * self.sectors_per_cluster
    }

    /// Absolute image byte offset of a data cluster (cluster >= 2).
    pub fn cluster_offset(&self, cluster: u32) -> u64 {
        let sector =
            self.cluster_heap_offset_sectors + (cluster as u64 - 2) * self.sectors_per_cluster;
        self.base_offset + sector * self.bytes_per_sector
    }

    fn fat_offset(&self) -> u64 {
        self.base_offset + self.fat_offset_sectors * self.bytes_per_sector
    }
}

/// One decoded exFAT directory entry.
#[derive(Debug, Clone)]
pub struct RecoveredEntry {
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub first_cluster: u32,
    pub no_fat_chain: bool,
    pub timestamps: Mactb,
    pub runs: Vec<FsDataRun>,
}

/// exFAT FAT (32-bit entries) for chain following / free detection.
pub struct ExfatFat {
    entries: Vec<u32>,
}

impl ExfatFat {
    pub fn load(image: &Arc<DiskImage>, geom: &ExfatGeom) -> Result<Self> {
        let len = (geom.fat_length_sectors * geom.bytes_per_sector) as usize;
        let raw = image.read_range(geom.fat_offset(), len)?;
        let count = (geom.cluster_count as usize + 2).min(raw.len() / 4);
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            let off = i * 4;
            entries.push(u32::from_le_bytes([
                raw[off],
                raw[off + 1],
                raw[off + 2],
                raw[off + 3],
            ]));
        }
        Ok(Self { entries })
    }

    fn next_cluster(&self, cluster: u32) -> u32 {
        self.entries.get(cluster as usize).copied().unwrap_or(0)
    }

    fn is_end_of_chain(&self, cluster: u32) -> bool {
        self.next_cluster(cluster) >= 0xFFFF_FFF8
    }
}

/// Build the absolute byte data runs for an exFAT file. When `no_fat_chain` is
/// set the file is contiguous (single run); otherwise the FAT chain is walked.
pub fn data_runs(
    geom: &ExfatGeom,
    fat: Option<&ExfatFat>,
    first_cluster: u32,
    size: u64,
    no_fat_chain: bool,
) -> Vec<FsDataRun> {
    if first_cluster < 2 || size == 0 {
        return Vec::new();
    }
    let cluster_size = geom.cluster_size();
    if cluster_size == 0 {
        return Vec::new();
    }
    let max_cluster = 2 + geom.cluster_count as u64;
    if no_fat_chain || fat.is_none() {
        let clusters_needed = size.div_ceil(cluster_size);
        if (first_cluster as u64) >= max_cluster {
            return Vec::new();
        }
        let available = max_cluster - first_cluster as u64;
        let clusters = clusters_needed.min(available);
        return vec![FsDataRun {
            logical_offset: 0,
            physical_offset: geom.cluster_offset(first_cluster),
            length: (clusters * cluster_size).min(size),
        }];
    }

    // Walk the FAT chain, coalescing physically-contiguous clusters into runs.
    let Some(fat) = fat else {
        return Vec::new();
    };
    let mut runs: Vec<FsDataRun> = Vec::new();
    let mut cluster = first_cluster;
    let mut logical = 0u64;
    let mut remaining = size;
    let mut guard = 0usize;
    while cluster >= 2
        && (cluster as u64) < max_cluster
        && remaining > 0
        && guard < MAX_DIR_CLUSTERS
    {
        let phys = geom.cluster_offset(cluster);
        let take = cluster_size.min(remaining);
        if let Some(last) = runs.last_mut() {
            if last.physical_offset + last.length == phys {
                last.length += take;
            } else {
                runs.push(FsDataRun {
                    logical_offset: logical,
                    physical_offset: phys,
                    length: take,
                });
            }
        } else {
            runs.push(FsDataRun {
                logical_offset: logical,
                physical_offset: phys,
                length: take,
            });
        }
        logical += take;
        remaining = remaining.saturating_sub(take);
        // Account the end-of-chain cluster's bytes before stopping (above),
        // then break — checking EOC at the loop head would truncate the file
        // by one cluster.
        if fat.is_end_of_chain(cluster) {
            break;
        }
        let next = fat.next_cluster(cluster);
        if next == cluster || (next as u64) < 2 {
            break;
        }
        cluster = next;
        guard += 1;
    }
    runs
}

/// Read a directory's raw bytes by following its cluster chain (or treating it
/// as contiguous if the FAT is unavailable).
pub fn read_dir_bytes(
    image: &Arc<DiskImage>,
    geom: &ExfatGeom,
    fat: &ExfatFat,
    first_cluster: u32,
) -> Result<Vec<u8>> {
    if first_cluster < 2 {
        anyhow::bail!("exFAT directory first cluster invalid");
    }
    let mut out = Vec::new();
    let cluster_size = geom.cluster_size() as usize;
    let max_cluster = 2 + geom.cluster_count;
    let mut cluster = first_cluster;
    let mut guard = 0usize;
    while cluster >= 2 && cluster < max_cluster && guard < MAX_DIR_CLUSTERS {
        let bytes = image.read_range(geom.cluster_offset(cluster), cluster_size)?;
        if bytes.is_empty() {
            break;
        }
        out.extend_from_slice(&bytes);
        // Read the end-of-chain cluster before stopping — otherwise a
        // single-cluster directory (FAT entry already EOC) yields nothing and
        // every multi-cluster directory drops its last cluster.
        if fat.is_end_of_chain(cluster) {
            break;
        }
        let next = fat.next_cluster(cluster);
        if next == cluster || next < 2 {
            break;
        }
        cluster = next;
        guard += 1;
    }
    Ok(out)
}

/// Lenient directory scan recovering **deleted** entry sets. Unlike the
/// vendored crate this does not stop at the first `0x00` end-marker (deleted
/// sets survive in slack past it) and never hard-errors on a stray secondary
/// entry — it resets and continues (TSK `exfatfs_dent.c` lenient mode, P1-8).
pub fn scan_dir_deleted(
    dir_bytes: &[u8],
    geom: &ExfatGeom,
    fat: Option<&ExfatFat>,
) -> Vec<RecoveredEntry> {
    scan_dir(dir_bytes, geom, fat, RecoverMode::DeletedOnly)
}

/// Same lenient walk for **live** entry sets. Used to resolve paths and locate
/// file data directly from raw directory entries.
pub fn scan_dir_live(
    dir_bytes: &[u8],
    geom: &ExfatGeom,
    fat: Option<&ExfatFat>,
) -> Vec<RecoveredEntry> {
    scan_dir(dir_bytes, geom, fat, RecoverMode::LiveOnly)
}

#[derive(Clone, Copy, PartialEq)]
enum RecoverMode {
    DeletedOnly,
    LiveOnly,
}

fn scan_dir(
    dir_bytes: &[u8],
    geom: &ExfatGeom,
    fat: Option<&ExfatFat>,
    mode: RecoverMode,
) -> Vec<RecoveredEntry> {
    let mut out = Vec::new();
    let slots = dir_bytes.len() / SLOT_LEN;
    let mut i = 0usize;
    while i < slots {
        let slot = &dir_bytes[i * SLOT_LEN..(i + 1) * SLOT_LEN];
        let raw_type = slot[0];
        let base_type = raw_type | IN_USE_BIT; // normalise the in-use bit
        let in_use = raw_type & IN_USE_BIT != 0;

        if raw_type == 0x00 {
            // End-of-directory marker. Live scans honour it; deleted-entry
            // scans continue into slack (lenient).
            if mode == RecoverMode::LiveOnly {
                break;
            }
            i += 1;
            continue;
        }

        if base_type != TYPE_FILE {
            // Not a primary File entry — a stray secondary or other type.
            // Skip leniently instead of aborting.
            i += 1;
            continue;
        }

        let want_deleted = mode == RecoverMode::DeletedOnly;
        if in_use == want_deleted {
            // Wrong allocation state for this mode; skip this set's primary.
            i += 1;
            continue;
        }

        // Parse the entry set starting at this File entry.
        match parse_entry_set(dir_bytes, i, geom, fat) {
            Some((entry, consumed)) => {
                if !entry.name.is_empty() && entry.name != "." && entry.name != ".." {
                    out.push(entry);
                }
                i += consumed.max(1);
            }
            None => {
                i += 1;
            }
        }
    }
    out
}

/// Parse the entry set whose File entry is at slot index `start`. Returns the
/// recovered entry plus how many 32-byte slots it consumed.
fn parse_entry_set(
    dir_bytes: &[u8],
    start: usize,
    geom: &ExfatGeom,
    fat: Option<&ExfatFat>,
) -> Option<(RecoveredEntry, usize)> {
    let file_slot = slot_at(dir_bytes, start)?;
    let secondary_count = file_slot[1] as usize;
    let attrs = u16::from_le_bytes([file_slot[4], file_slot[5]]);
    let is_directory = attrs & ATTR_DIRECTORY != 0;
    let timestamps = file_entry_timestamps(file_slot);

    // Stream Extension must immediately follow.
    let stream = slot_at(dir_bytes, start + 1)?;
    if stream[0] | IN_USE_BIT != TYPE_STREAM {
        return None;
    }
    let flags = stream[1];
    let no_fat_chain = flags & NO_FAT_CHAIN != 0;
    let name_len = stream[3] as usize;
    let size = u64::from_le_bytes([
        stream[24], stream[25], stream[26], stream[27], stream[28], stream[29], stream[30],
        stream[31],
    ]);
    let first_cluster = u32::from_le_bytes([stream[20], stream[21], stream[22], stream[23]]);

    // File Name entries follow; each carries up to 15 UTF-16 chars.
    let mut units: Vec<u16> = Vec::new();
    let name_entries = secondary_count.saturating_sub(1);
    for k in 0..name_entries {
        let Some(name_slot) = slot_at(dir_bytes, start + 2 + k) else {
            break;
        };
        if name_slot[0] | IN_USE_BIT != TYPE_NAME {
            break;
        }
        for c in 0..15 {
            let off = 2 + c * 2;
            let unit = u16::from_le_bytes([name_slot[off], name_slot[off + 1]]);
            units.push(unit);
        }
    }
    units.truncate(name_len.min(units.len()));
    let name = String::from_utf16_lossy(&units)
        .trim_end_matches('\u{0}')
        .to_string();
    if name.is_empty() {
        return None;
    }

    let runs = if is_directory {
        Vec::new()
    } else {
        data_runs(geom, fat, first_cluster, size, no_fat_chain)
    };

    let consumed = 1 + secondary_count; // file entry + secondaries
    Some((
        RecoveredEntry {
            name,
            is_directory,
            size: if is_directory { 0 } else { size },
            first_cluster,
            no_fat_chain,
            timestamps,
            runs,
        },
        consumed,
    ))
}

fn slot_at(dir_bytes: &[u8], index: usize) -> Option<&[u8]> {
    dir_bytes.get(index * SLOT_LEN..(index + 1) * SLOT_LEN)
}

fn file_entry_timestamps(slot: &[u8]) -> Mactb {
    let created = u32::from_le_bytes([slot[8], slot[9], slot[10], slot[11]]);
    let modified = u32::from_le_bytes([slot[12], slot[13], slot[14], slot[15]]);
    let accessed = u32::from_le_bytes([slot[16], slot[17], slot[18], slot[19]]);
    // Time-zone offsets (15-min units) at 22/23/24 — applied for parity with
    // the live adapter's `exfat_ts_to_rfc3339`. Each byte is bit7 = OffsetValid
    // and bits6-0 = the offset; an invalid byte means no offset (UTC). Decode
    // identically to the vendored exFAT crate (`vendor/exfat/src/entries.rs`)
    // so recovered MAC times match the live listing instead of reading the raw
    // byte (e.g. 0x80 → -128 → +32h skew on every Windows-written UTC stamp).
    let created_tz = decode_utc_offset(slot[22]);
    let modified_tz = decode_utc_offset(slot[23]);
    let accessed_tz = decode_utc_offset(slot[24]);
    let created = exfat_timestamp(created, created_tz);
    let modified = exfat_timestamp(modified, modified_tz);
    Mactb {
        modified,
        accessed: exfat_timestamp(accessed, accessed_tz),
        created: created.clone(),
        mft_changed: None,
        birth: created,
    }
}

/// Decode an exFAT UTCOffset byte: bit7 is the OffsetValid flag and bits6-0 the
/// offset in 15-minute units. An invalid byte means no recorded offset (treat
/// as UTC). Decoded identically to the vendored exFAT crate
/// (`vendor/exfat/src/entries.rs`) so recovered timestamps match the live
/// listing — reading the whole byte as a raw `i8` instead (e.g. 0x80 → -128)
/// skews every Windows-written UTC stamp by +32h.
fn decode_utc_offset(byte: u8) -> i8 {
    if (byte >> 7) & 1 == 1 {
        (byte & 0x7F) as i8
    } else {
        0
    }
}

fn exfat_timestamp(packed: u32, tz: i8) -> Option<String> {
    if packed == 0 {
        return None;
    }
    let year = 1980 + ((packed >> 25) & 0x7F) as i32;
    let month = (packed >> 21) & 0x0F;
    let day = (packed >> 16) & 0x1F;
    let hour = (packed >> 11) & 0x1F;
    let minute = (packed >> 5) & 0x3F;
    let second = (packed & 0x1F) * 2;
    let naive =
        NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second.min(59))?;
    // The tz byte is a signed count of 15-minute units when its 0x80 valid bit
    // is set; mirror the live adapter which subtracts 15-min*offset.
    let dt = Utc.from_utc_datetime(&naive) - Duration::minutes(15 * i64::from(tz));
    Some(dt.to_rfc3339())
}

/// Read a partition's boot sector and parse exFAT geometry.
///
/// Like FAT32, exFAT keeps a Backup Boot Region (sector 12 mirrors the main VBR
/// at sector 0). A wiped or corrupted primary VBR would otherwise make recovery
/// silently yield nothing, so fall back to the sector-12 backup when the primary
/// is not a structurally valid exFAT boot sector. Mirrors
/// `volumes::fat_deleted::open_geom`.
pub fn open_geom(image: &Arc<DiskImage>, base_offset: u64) -> Result<ExfatGeom> {
    let boot = read_effective_boot_sector(image, base_offset)?;
    ExfatGeom::parse(&boot, base_offset)
}

fn read_effective_boot_sector(image: &Arc<DiskImage>, base_offset: u64) -> Result<[u8; 512]> {
    let primary = image
        .read_range(base_offset, BOOT_SECTOR_LEN)
        .map_err(|err| anyhow!("exFAT boot sector read: {err}"))?;
    if exfat_boot_looks_valid(&primary) {
        return boot_sector_array(&primary);
    }
    // The backup VBR is a faithful copy: every region offset still resolves
    // relative to `base_offset`, only the BPB bytes come from sector 12.
    let backup_offset = base_offset + EXFAT_BACKUP_BOOT_SECTOR * image.sector_size as u64;
    if let Ok(backup) = image.read_range(backup_offset, BOOT_SECTOR_LEN) {
        if exfat_boot_looks_valid(&backup) {
            return boot_sector_array(&backup);
        }
    }
    boot_sector_array(&primary)
}

/// An intact exFAT boot sector carries the `"EXFAT   "` filesystem name at
/// offset 3, the `0x55AA` boot signature, and in-range geometry shifts (the same
/// bounds `ExfatGeom::parse` enforces).
fn exfat_boot_looks_valid(boot: &[u8]) -> bool {
    boot.len() >= BOOT_SECTOR_LEN
        && &boot[3..11] == b"EXFAT   "
        && boot[510..512] == [0x55, 0xAA]
        && boot[108] <= 12
        && boot[109] <= 25
}

fn boot_sector_array(bytes: &[u8]) -> Result<[u8; 512]> {
    let slice = bytes
        .get(..BOOT_SECTOR_LEN)
        .ok_or_else(|| anyhow!("exFAT boot sector too short"))?;
    let mut out = [0u8; BOOT_SECTOR_LEN];
    out.copy_from_slice(slice);
    Ok(out)
}

/// Resolve an absolute volume path to the directory's first cluster and
/// `no_fat_chain` flag by walking live entry sets from the root. Returns the
/// root cluster for the empty/`/` path.
pub fn resolve_dir(
    image: &Arc<DiskImage>,
    geom: &ExfatGeom,
    fat: &ExfatFat,
    path: &str,
) -> Result<u32> {
    let mut cluster = geom.root_cluster;
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        let dir_bytes = read_dir_bytes(image, geom, fat, cluster)?;
        let mut found = None;
        for entry in scan_dir_live(&dir_bytes, geom, Some(fat)) {
            if entry.is_directory && entry.name.eq_ignore_ascii_case(segment) {
                found = Some(entry.first_cluster);
                break;
            }
        }
        match found {
            Some(next) if next >= 2 => cluster = next,
            _ => anyhow::bail!("exFAT directory segment not found: {segment}"),
        }
    }
    Ok(cluster)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_offset_decodes_valid_bit_and_offset() {
        // Invalid (bit7 clear) → no offset.
        assert_eq!(decode_utc_offset(0x00), 0);
        assert_eq!(decode_utc_offset(0x20), 0);
        // 0x80 = valid + zero offset (Windows writes this for UTC).
        assert_eq!(decode_utc_offset(0x80), 0);
        // 0xA0 = valid + 0x20 (32 × 15 min = UTC+8).
        assert_eq!(decode_utc_offset(0xA0), 32);
    }

    #[test]
    fn utc_timestamp_no_skew_for_windows_utc_byte() {
        // packed = 2021-06-15 12:00:00. tz byte 0x80 → offset 0 → no shift.
        let packed: u32 = (((2021 - 1980) as u32) << 25) | (6 << 21) | (15 << 16) | (12 << 11);
        let ts = exfat_timestamp(packed, decode_utc_offset(0x80)).expect("timestamp");
        assert!(ts.starts_with("2021-06-15T12:00:00"), "unexpected ts: {ts}");
    }

    fn geom() -> ExfatGeom {
        ExfatGeom {
            base_offset: 0,
            fat_offset_sectors: 8,
            fat_length_sectors: 8,
            cluster_heap_offset_sectors: 64,
            cluster_count: 1000,
            root_cluster: 4,
            bytes_per_sector: 512,
            sectors_per_cluster: 1,
        }
    }

    fn file_entry(secondary: u8, attrs: u16) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = TYPE_FILE;
        s[1] = secondary;
        s[4..6].copy_from_slice(&attrs.to_le_bytes());
        s
    }

    fn stream_entry(name_len: u8, first_cluster: u32, size: u64, no_fat: bool) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = TYPE_STREAM;
        s[1] = if no_fat { NO_FAT_CHAIN } else { 0 };
        s[3] = name_len;
        s[20..24].copy_from_slice(&first_cluster.to_le_bytes());
        s[24..32].copy_from_slice(&size.to_le_bytes());
        s
    }

    fn name_entry(chars: &[u16]) -> [u8; 32] {
        let mut s = [0u8; 32];
        s[0] = TYPE_NAME;
        for (i, &c) in chars.iter().take(15).enumerate() {
            let off = 2 + i * 2;
            s[off] = (c & 0xFF) as u8;
            s[off + 1] = (c >> 8) as u8;
        }
        s
    }

    fn mark_deleted(slot: &mut [u8; 32]) {
        slot[0] &= !IN_USE_BIT;
    }

    #[test]
    fn recovers_deleted_entry_set_contiguous() {
        let g = geom();
        let name: Vec<u16> = "secret.bin".encode_utf16().collect();
        let mut file = file_entry(2, 0x20);
        let mut stream = stream_entry(name.len() as u8, 10, 5000, true);
        let mut nm = name_entry(&name);
        mark_deleted(&mut file);
        mark_deleted(&mut stream);
        mark_deleted(&mut nm);
        let mut buf = Vec::new();
        buf.extend_from_slice(&file);
        buf.extend_from_slice(&stream);
        buf.extend_from_slice(&nm);
        let rec = scan_dir_deleted(&buf, &g, None);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].name, "secret.bin");
        assert_eq!(rec[0].size, 5000);
        assert!(rec[0].no_fat_chain);
        assert_eq!(rec[0].runs.len(), 1);
        // 5000 bytes over 512-byte clusters but length clamped to size.
        assert_eq!(rec[0].runs[0].length, 5000);
        assert_eq!(rec[0].runs[0].physical_offset, g.cluster_offset(10));
    }

    #[test]
    fn lenient_scan_continues_past_end_marker() {
        let g = geom();
        let name: Vec<u16> = "after.txt".encode_utf16().collect();
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 32]); // 0x00 end marker
        let mut file = file_entry(2, 0x20);
        let mut stream = stream_entry(name.len() as u8, 12, 100, true);
        let mut nm = name_entry(&name);
        mark_deleted(&mut file);
        mark_deleted(&mut stream);
        mark_deleted(&mut nm);
        buf.extend_from_slice(&file);
        buf.extend_from_slice(&stream);
        buf.extend_from_slice(&nm);
        let rec = scan_dir_deleted(&buf, &g, None);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].name, "after.txt");
    }

    #[test]
    fn live_scan_skips_deleted_and_honours_end_marker() {
        let g = geom();
        let name: Vec<u16> = "live.dat".encode_utf16().collect();
        let mut buf = Vec::new();
        // live set
        buf.extend_from_slice(&file_entry(2, 0x20));
        buf.extend_from_slice(&stream_entry(name.len() as u8, 20, 64, true));
        buf.extend_from_slice(&name_entry(&name));
        buf.extend_from_slice(&[0u8; 32]); // end marker
        let rec = scan_dir_live(&buf, &g, None);
        assert_eq!(rec.len(), 1);
        assert_eq!(rec[0].name, "live.dat");
    }

    #[test]
    fn parses_exfat_geometry() {
        let mut boot = vec![0u8; 512];
        boot[3..11].copy_from_slice(b"EXFAT   ");
        boot[80..84].copy_from_slice(&2048u32.to_le_bytes());
        boot[84..88].copy_from_slice(&512u32.to_le_bytes());
        boot[88..92].copy_from_slice(&4096u32.to_le_bytes());
        boot[92..96].copy_from_slice(&100000u32.to_le_bytes());
        boot[96..100].copy_from_slice(&5u32.to_le_bytes());
        boot[108] = 9; // 512 bytes/sector
        boot[109] = 3; // 8 sectors/cluster
        let g = ExfatGeom::parse(&boot, 2048).unwrap();
        assert_eq!(g.bytes_per_sector, 512);
        assert_eq!(g.sectors_per_cluster, 8);
        assert_eq!(g.root_cluster, 5);
        assert_eq!(g.cluster_count, 100000);
    }
}
