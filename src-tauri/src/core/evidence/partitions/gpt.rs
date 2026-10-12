//! GPT detection.
//!
//! The standard layout (header at LBA 1) is delegated to the vendored `gpt`
//! crate (`vendor/gpt`): it reads the primary header, validates header CRC32
//! and partition-table CRC32, parses every entry's type GUID + UTF-16 name,
//! and exposes the canonical `partition_types::*` registry. We only translate
//! its `Partition` records into our `Partition`/`PartitionKind` enum.
//!
//! When the library refuses an image — bad CRC, unusual entry size, or no
//! header at LBA 1 — we fall back to a permissive manual scan using
//! a multi-LBA candidate list (`1, 0, 7, 8`) and the strict
//! 128-byte entry size check. Forensic images frequently have the GPT
//! signature without consistent CRCs. GPT is checked before MBR so these
//! images are not misread as ordinary DOS partition tables.

use std::io::{self, Read, Seek, SeekFrom};

use anyhow::Result;
use binrw::BinRead;
use uuid::Uuid;

use crate::core::evidence::partitions::{
    DetectCtx, PartParser, Partition, PartitionKind, SectorSource, TableScheme,
};

const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
const GPT_ENTRY_SIZE: u32 = 128;
const GPT_HEADER_BYTES: usize = 92;
/// Forensic candidate LBAs to probe when the library rejects LBA 1.
const GPT_FALLBACK_LBAS: [u64; 4] = [1, 0, 7, 8];
/// Upper bound on the bytes read for the partition-entry array (P1-10). A
/// standard table is 128 × 128 = 16 KiB; the 1 MiB cap (8192 entries) keeps a
/// corrupt `num_parts` from triggering a multi-GiB allocation while still
/// reading the entire array straight from the source rather than the 16-sector
/// head probe — which only covers ~56 of the 128 entries.
const GPT_ENTRY_ARRAY_CAP: usize = 1024 * 1024;

/// BitLocker partition GUID (`AEAFCA3B-61C1-40E0-87A3-47D1AF93A9FC`). Not
/// present in `gpt::partition_types` so we keep our own constant.
const BITLOCKER_GUID: Uuid = Uuid::from_bytes([
    0xAE, 0xAF, 0xCA, 0x3B, 0x61, 0xC1, 0x40, 0xE0, 0x87, 0xA3, 0x47, 0xD1, 0xAF, 0x93, 0xA9, 0xFC,
]);

/// FreeBSD UFS partition GUID (`516E7CB6-6ECF-11D6-8FF8-00022D09712B`). FreeBSD
/// installer images put the rootfs on a partition with this type.
const FREEBSD_UFS_GUID: Uuid = Uuid::from_bytes([
    0x51, 0x6E, 0x7C, 0xB6, 0x6E, 0xCF, 0x11, 0xD6, 0x8F, 0xF8, 0x00, 0x02, 0x2D, 0x09, 0x71, 0x2B,
]);

/// FreeBSD swap partition GUID (`516E7CB5-6ECF-11D6-8FF8-00022D09712B`).
const FREEBSD_SWAP_GUID: Uuid = Uuid::from_bytes([
    0x51, 0x6E, 0x7C, 0xB5, 0x6E, 0xCF, 0x11, 0xD6, 0x8F, 0xF8, 0x00, 0x02, 0x2D, 0x09, 0x71, 0x2B,
]);

/// FreeBSD ZFS partition GUID (`516E7CBA-6ECF-11D6-8FF8-00022D09712B`). FreeBSD
/// installer images put the rootfs ZFS pool on a partition with this type.
const FREEBSD_ZFS_GUID: Uuid = Uuid::from_bytes([
    0x51, 0x6E, 0x7C, 0xBA, 0x6E, 0xCF, 0x11, 0xD6, 0x8F, 0xF8, 0x00, 0x02, 0x2D, 0x09, 0x71, 0x2B,
]);

/// Solaris / illumos / Linux ZFS partition GUID
/// (`6A898CC3-1DD2-11B2-99A6-080020736631`). Originally registered by Solaris
/// for the `/usr` slice, this same GUID was adopted by OpenZFS and is what
/// ZFS-on-Linux writes when `zpool create` partitions a whole disk. Apple's
/// historical macOS ZFS port also used it.
const SOLARIS_ZFS_GUID: Uuid = Uuid::from_bytes([
    0x6A, 0x89, 0x8C, 0xC3, 0x1D, 0xD2, 0x11, 0xB2, 0x99, 0xA6, 0x08, 0x00, 0x20, 0x73, 0x66, 0x31,
]);

/// MidnightBSD ZFS partition GUID (`85D5E45D-237C-11E1-B4B3-E89A8F7FC3A7`).
const MIDNIGHTBSD_ZFS_GUID: Uuid = Uuid::from_bytes([
    0x85, 0xD5, 0xE4, 0x5D, 0x23, 0x7C, 0x11, 0xE1, 0xB4, 0xB3, 0xE8, 0x9A, 0x8F, 0x7F, 0xC3, 0xA7,
]);

/// Apple HFS+ partition GUID (`48465300-0000-11AA-AA11-00306543ECAC`). Used
/// for both HFS+ (case-insensitive) and HFSX (case-sensitive) on GPT-labeled
/// Apple disks; the case-sensitivity bit lives in the volume header at
/// offset 0x400, so we always emit `HfsPlus` here and let
/// `refine_partition_kinds` upgrade to `HfsX` when it probes the boot region.
const APPLE_HFS_GUID: Uuid = Uuid::from_bytes([
    0x48, 0x46, 0x53, 0x00, 0x00, 0x00, 0x11, 0xAA, 0xAA, 0x11, 0x00, 0x30, 0x65, 0x43, 0xEC, 0xAC,
]);

pub struct GptParser;

impl PartParser for GptParser {
    fn name(&self) -> &'static str {
        "gpt"
    }

    fn scheme(&self) -> TableScheme {
        TableScheme::Gpt
    }

    fn try_parse(&self, ctx: &DetectCtx<'_>) -> Result<Option<Vec<Partition>>> {
        if has_gpt_signature_at(ctx, 1) {
            if let Some(out) = parse_with_gpt_crate(ctx) {
                return Ok(Some(out));
            }
        }
        for &lba in &GPT_FALLBACK_LBAS {
            if has_gpt_signature_at(ctx, lba) {
                if let Some(out) = parse_manual(ctx, lba) {
                    return Ok(Some(out));
                }
            }
        }
        // Primary header (and the forensic leading-LBA candidates) failed or
        // produced an out-of-bounds table. Fall back to the backup GPT header
        // mirrored at the last LBA of the disk — `gpt.c` does the same after
        // exhausting the primary table and alternate sector sizes (P1-9).
        if let Some(out) = parse_backup(ctx) {
            return Ok(Some(out));
        }
        Ok(None)
    }
}

/// Read the disk GUID (header offset 56) from whichever GPT header location
/// holds a valid signature, probing the same candidates as `try_parse`: the
/// forensic leading LBAs first, then the backup header at the last LBA. The
/// entry-locator fields are validated via `parse_header_fields` so a stray
/// `EFI PART` signature with garbage at offset 56 cannot mint a bogus GUID.
/// Best effort — `None` when no plausible header is readable or the GUID is
/// all-zero.
pub(crate) fn read_disk_guid(ctx: &DetectCtx<'_>) -> Option<String> {
    let mut candidates: Vec<u64> = GPT_FALLBACK_LBAS.to_vec();
    if let Some(max_sector) = max_sector(ctx) {
        if max_sector >= 3 {
            candidates.push(max_sector - 1);
        }
    }
    for lba in candidates {
        let offset = lba.checked_mul(ctx.sector_size as u64)?;
        let Ok(header) = ctx.source.read(offset, GPT_HEADER_BYTES) else {
            continue;
        };
        let Some(fields) = parse_header_fields(&header) else {
            continue;
        };
        let Some(guid) = uuid_from_disk_bytes(&fields.disk_guid) else {
            continue;
        };
        if !guid.is_nil() {
            return Some(guid.to_string().to_uppercase());
        }
    }
    None
}

fn has_gpt_signature_at(ctx: &DetectCtx<'_>, lba: u64) -> bool {
    let Some(start) = (lba as usize).checked_mul(ctx.sector_size) else {
        return false;
    };
    let end = start.saturating_add(GPT_SIGNATURE.len());
    if let Some(slice) = ctx.head.get(start..end) {
        return slice == GPT_SIGNATURE;
    }
    // The candidate LBA falls past the 16-sector head probe — true for 4Kn
    // LBA 7/8 (bytes 28672/32768 > 0x2000). Read the signature straight from
    // the source so the forensic leading-LBA recovery is not silently dead on
    // 4Kn images.
    match ctx.source.read(start as u64, GPT_SIGNATURE.len()) {
        Ok(bytes) => bytes == GPT_SIGNATURE,
        Err(_) => false,
    }
}

/// Drive the vendored `gpt` crate against the standard LBA-1 layout. Returns
/// `None` when the library can't make sense of the disk (unsupported sector
/// size, header CRC mismatch, partition CRC mismatch, non-128-byte entries) so
/// the caller can fall through to the permissive manual scanner.
fn parse_with_gpt_crate(ctx: &DetectCtx<'_>) -> Option<Vec<Partition>> {
    let lb_size = sector_size_to_lb(ctx.sector_size)?;
    let mut device = SectorCursor::new(ctx.source);
    let header = gpt::header::read_header_from_arbitrary_device(&mut device, lb_size).ok()?;
    if header.part_size != GPT_ENTRY_SIZE {
        return None;
    }
    let parts = gpt::partition::file_read_partitions(&mut device, &header, lb_size).ok()?;
    let mut out = Vec::with_capacity(parts.len());
    for (_id, p) in parts.into_iter() {
        if p.last_lba <= p.first_lba {
            continue;
        }
        let Some(length_lba) = p
            .last_lba
            .checked_sub(p.first_lba)
            .and_then(|span| span.checked_add(1))
        else {
            continue;
        };
        out.push(Partition {
            // Sequential over emitted partitions — see `parse_entries`.
            id: out.len() as u8,
            kind: guid_to_kind(p.part_type_guid.guid),
            start_lba: p.first_lba,
            length_lba,
            label: if p.name.is_empty() {
                None
            } else {
                Some(p.name)
            },
            unique_guid: (!p.part_guid.is_nil()).then(|| *p.part_guid.as_bytes()),
        });
    }
    // The crate already validated both CRCs, so the table is genuine: clamp
    // any entry past the image end (e.g. a 4Kn image opened at 512) rather
    // than rejecting the whole — and a recognised-but-empty table is still a
    // GPT match, so always return `Some`.
    Some(clamp_to_image(out, max_sector(ctx)))
}

/// Permissive manual entry walker: read partition_entry_lba / num_entries / entry_size from the
/// 92-byte header at the candidate LBA, then read and iterate the entry array
/// directly from the source (P1-10) — the 16-sector head probe only covers
/// ~56 of a standard 128-entry table, so entries 57..128 were silently lost.
/// No CRC validation — forensic images frequently disagree with their own
/// checksums — but out-of-bounds tables are rejected so a stray `EFI PART`
/// in a random sector does not yield garbage partitions (P1-11).
fn parse_manual(ctx: &DetectCtx<'_>, header_lba: u64) -> Option<Vec<Partition>> {
    let header_off = header_lba.checked_mul(ctx.sector_size as u64)?;
    let header = ctx.source.read(header_off, GPT_HEADER_BYTES).ok()?;
    let fields = parse_header_fields(&header)?;
    let entries = read_entry_array(ctx, &fields)?;
    let parts = parse_entries(&entries, fields.num_parts, fields.entry_size);
    enforce_image_bounds(parts, max_sector(ctx))
}

/// Backup-header fallback (P1-9). The backup GPT header is mirrored at the last
/// LBA of the disk and points at its own entry-array copy near the disk tail.
/// When the primary header (and the forensic leading-LBA candidates) are
/// unreadable or out of bounds, recover the table from the backup the way
/// `gpt.c` reads `(size/bs) - 1` after exhausting the primary.
fn parse_backup(ctx: &DetectCtx<'_>) -> Option<Vec<Partition>> {
    let max_sector = max_sector(ctx)?;
    // lba0: protective MBR, lba1: primary header, .., last lba: backup header.
    if max_sector < 3 {
        return None;
    }
    let backup_lba = max_sector - 1;
    let header_off = backup_lba.checked_mul(ctx.sector_size as u64)?;
    let header = ctx.source.read(header_off, GPT_HEADER_BYTES).ok()?;
    let fields = parse_header_fields(&header)?;
    let entries = read_entry_array(ctx, &fields)?;
    let parts = parse_entries(&entries, fields.num_parts, fields.entry_size);
    enforce_image_bounds(parts, Some(max_sector))
}

struct GptHeaderFields {
    disk_guid: [u8; 16],
    part_start_lba: u64,
    num_parts: u32,
    entry_size: u32,
}

#[derive(Debug, BinRead)]
#[br(little, magic = b"EFI PART")]
struct RawGptHeaderFields {
    #[br(seek_before = SeekFrom::Start(56))]
    disk_guid: [u8; 16],
    part_start_lba: u64,
    num_parts: u32,
    entry_size: u32,
}

#[derive(Debug, BinRead)]
#[br(little)]
struct RawGptEntry {
    type_guid: [u8; 16],
    unique_guid: [u8; 16],
    first_lba: u64,
    last_lba: u64,
    _attributes: u64,
    name: [u8; 72],
}

/// Validate the `EFI PART` signature and pull the entry-array locator fields
/// out of a 92-byte GPT header. Returns `None` for a missing signature, a
/// non-128-byte entry size, or a zero entry count.
fn parse_header_fields(header: &[u8]) -> Option<GptHeaderFields> {
    if header.len() < GPT_HEADER_BYTES {
        return None;
    }
    let raw = RawGptHeaderFields::read(&mut io::Cursor::new(header)).ok()?;
    if raw.entry_size != GPT_ENTRY_SIZE || raw.num_parts == 0 {
        return None;
    }
    Some(GptHeaderFields {
        disk_guid: raw.disk_guid,
        part_start_lba: raw.part_start_lba,
        num_parts: raw.num_parts,
        entry_size: raw.entry_size,
    })
}

/// Read the partition-entry array straight from the source at the LBA the
/// header points to, capped at [`GPT_ENTRY_ARRAY_CAP`] so a corrupt
/// `num_parts` cannot trigger a huge allocation.
fn read_entry_array(ctx: &DetectCtx<'_>, fields: &GptHeaderFields) -> Option<Vec<u8>> {
    let entries_start = fields.part_start_lba.checked_mul(ctx.sector_size as u64)?;
    if entries_start >= ctx.size {
        return None;
    }
    let want = (fields.num_parts as u64)
        .checked_mul(fields.entry_size as u64)?
        .min(GPT_ENTRY_ARRAY_CAP as u64) as usize;
    let bytes = ctx.source.read(entries_start, want).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

/// Maximum addressable sector of the image (`size / sector_size`), used as the
/// out-of-bounds threshold for partition starts. `None` for a zero sector size.
fn max_sector(ctx: &DetectCtx<'_>) -> Option<u64> {
    (ctx.sector_size != 0).then(|| ctx.size / ctx.sector_size as u64)
}

/// Filter out-of-image entries from a CRC-validated table. The `gpt` crate has
/// already proven the table genuine, so a stray entry past the image end (e.g.
/// a 4Kn image opened with a 512-byte sector assumption) is dropped rather than
/// rejecting the whole table.
fn clamp_to_image(parts: Vec<Partition>, max_sector: Option<u64>) -> Vec<Partition> {
    match max_sector {
        Some(max_sector) => parts
            .into_iter()
            .filter(|p| p.start_lba < max_sector)
            .collect(),
        None => parts,
    }
}

/// Bounds gate for the permissive (no-CRC) manual / backup paths (P1-11). Drops
/// entries past the image end and rejects the whole table when either of its
/// first two non-empty entries is out of bounds — `gpt.c`/`dos.c` apply the
/// same `i < 2` leading-entry sanity check so a random sector that merely
/// contains `EFI PART` cannot mint garbage partitions. Returns `None` (try the
/// next candidate / parser) when the table is rejected or empty.
fn enforce_image_bounds(parts: Vec<Partition>, max_sector: Option<u64>) -> Option<Vec<Partition>> {
    if let Some(max_sector) = max_sector {
        if parts.iter().take(2).any(|p| p.start_lba >= max_sector) {
            return None;
        }
    }
    let kept = clamp_to_image(parts, max_sector);
    (!kept.is_empty()).then_some(kept)
}

fn sector_size_to_lb(sector_size: usize) -> Option<gpt::disk::LogicalBlockSize> {
    match sector_size {
        512 => Some(gpt::disk::LogicalBlockSize::Lb512),
        4096 => Some(gpt::disk::LogicalBlockSize::Lb4096),
        _ => None,
    }
}

fn guid_to_kind(guid: Uuid) -> PartitionKind {
    use gpt::partition_types as t;
    if guid == t::LINUX_LVM.guid {
        PartitionKind::Lvm
    } else if guid == t::LINUX_SWAP.guid {
        PartitionKind::Swap
    } else if guid == t::BIOS.guid {
        PartitionKind::BiosBoot
    } else if guid == t::MACOS_APFS.guid {
        PartitionKind::ApfsContainer
    } else if guid == APPLE_HFS_GUID {
        PartitionKind::HfsPlus
    } else if guid == BITLOCKER_GUID {
        PartitionKind::Bitlocker
    } else if guid == FREEBSD_UFS_GUID {
        PartitionKind::Ufs
    } else if guid == FREEBSD_SWAP_GUID {
        PartitionKind::Swap
    } else if guid == FREEBSD_ZFS_GUID || guid == SOLARIS_ZFS_GUID || guid == MIDNIGHTBSD_ZFS_GUID {
        PartitionKind::Zfs
    } else {
        // Includes BASIC (`MS_BASIC_DATA`), LINUX_FS, MICROSOFT_RESERVED, EFI,
        // anything else: leave Unknown so `refine_partition_kinds` can probe
        // the boot region for an actual filesystem signature.
        PartitionKind::Unknown
    }
}

fn parse_entries(entries: &[u8], num_parts: u32, entry_size: u32) -> Vec<Partition> {
    let mut out = Vec::with_capacity((num_parts as usize).min(128));
    for index in 0..num_parts as usize {
        let Some(base) = index.checked_mul(entry_size as usize) else {
            break;
        };
        let end = base.saturating_add(entry_size as usize);
        if end > entries.len() {
            break;
        }
        let entry = &entries[base..end];
        let Some(raw) = RawGptEntry::read(&mut io::Cursor::new(entry)).ok() else {
            continue;
        };
        if raw.type_guid.iter().all(|&b| b == 0) {
            continue;
        }
        let Some(type_guid) = uuid_from_disk_bytes(&raw.type_guid) else {
            continue;
        };
        let first_lba = raw.first_lba;
        let last_lba = raw.last_lba;
        if last_lba <= first_lba {
            continue;
        }
        // Checked arithmetic: a corrupt / planted EndingLBA near u64::MAX
        // passes the `last_lba > first_lba` guard but would overflow
        // `last_lba - first_lba + 1` (debug panic, release wrap-to-0). Skip
        // such entries rather than minting a bogus zero-length partition —
        // this runs before `enforce_image_bounds`, so the bounds gate cannot
        // catch it.
        let Some(length_lba) = last_lba
            .checked_sub(first_lba)
            .and_then(|span| span.checked_add(1))
        else {
            continue;
        };
        out.push(Partition {
            // Sequential over emitted partitions, not the raw slot index — a
            // populated entry at slot ≥256 (the 1 MiB array cap allows up to
            // 8192 slots) would wrap `index as u8` and collide.
            id: out.len() as u8,
            kind: guid_to_kind(type_guid),
            start_lba: first_lba,
            length_lba,
            label: parse_utf16_name(&raw.name),
            // 唯一分区 GUID 在条目偏移 16（与类型 GUID 同样的磁盘混合端序），
            // 解码成 canonical 字节，便于与 MountedDevices 的 DMIO 项比对。
            unique_guid: uuid_from_disk_bytes(&raw.unique_guid)
                .filter(|u| !u.is_nil())
                .map(|u| *u.as_bytes()),
        });
    }
    out
}

/// Decode the on-disk GPT type-GUID byte order (3 little-endian fields + an
/// 8-byte big-endian tail) into a canonical [`Uuid`]. `pub(crate)` so the
/// drive-letter recovery (`core::analysis::identify::win_drive_letters`) can decode the GUID that
/// Windows stores in a `MountedDevices` `DMIO:ID:` value using the identical
/// byte convention.
pub(crate) fn uuid_from_disk_bytes(bytes: &[u8]) -> Option<Uuid> {
    let arr: [u8; 16] = bytes.try_into().ok()?;
    let d1 = u32::from_le_bytes([arr[0], arr[1], arr[2], arr[3]]);
    let d2 = u16::from_le_bytes([arr[4], arr[5]]);
    let d3 = u16::from_le_bytes([arr[6], arr[7]]);
    let d4 = [
        arr[8], arr[9], arr[10], arr[11], arr[12], arr[13], arr[14], arr[15],
    ];
    Some(Uuid::from_fields(d1, d2, d3, &d4))
}

fn parse_utf16_name(bytes: &[u8]) -> Option<String> {
    let chunks = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect::<Vec<_>>();
    if chunks.is_empty() {
        None
    } else {
        String::from_utf16(&chunks).ok()
    }
}

/// `Read + Seek` adapter around a `SectorSource`. The vendored `gpt` crate
/// only ever reads the primary GPT header (LBA 1) and the partition-entry
/// table starting at `header.part_start`, so the adapter pulls bytes through
/// the underlying source on demand instead of materialising the whole disk
/// into memory.
struct SectorCursor<'a> {
    source: &'a dyn SectorSource,
    pos: u64,
    size: u64,
}

impl<'a> SectorCursor<'a> {
    fn new(source: &'a dyn SectorSource) -> Self {
        Self {
            size: source.size(),
            source,
            pos: 0,
        }
    }
}

impl Read for SectorCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.size || buf.is_empty() {
            return Ok(0);
        }
        let want = (buf.len() as u64).min(self.size - self.pos) as usize;
        let bytes = self
            .source
            .read(self.pos, want)
            .map_err(|err| io::Error::other(err.to_string()))?;
        let copied = bytes.len();
        buf[..copied].copy_from_slice(&bytes);
        self.pos += copied as u64;
        Ok(copied)
    }
}

impl Seek for SectorCursor<'_> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(off) => off as i128,
            SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
            SeekFrom::End(delta) => self.size as i128 + delta as i128,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "GPT cursor seek before start",
            ));
        }
        self.pos = target as u64;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::evidence::partitions::{detect_partitions, test_support::BufSource};

    #[test]
    fn rejects_gpt_without_128_byte_entries() {
        let sector = 512usize;
        let mut buf = vec![0u8; sector * 34];
        buf[sector..sector + 8].copy_from_slice(GPT_SIGNATURE);
        buf[sector + 72..sector + 80].copy_from_slice(&2u64.to_le_bytes());
        buf[sector + 80..sector + 84].copy_from_slice(&128u32.to_le_bytes());
        buf[sector + 84..sector + 88].copy_from_slice(&64u32.to_le_bytes());
        buf[510] = 0x55;
        buf[511] = 0xAA;
        let src = BufSource(buf);
        let parts = detect_partitions(&src, 512).expect("detect");
        assert!(parts
            .iter()
            .all(|p| p.kind != PartitionKind::Unknown || p.start_lba == 0 && p.length_lba == 0));
    }

    /// CRC-less fixture: the library refuses on header-CRC mismatch so this
    /// exercises the manual fallback at LBA 1. The disk is large enough that
    /// the partition stays in bounds (P1-11).
    #[test]
    fn parses_gpt_with_entry_at_lba_2_via_manual_fallback() {
        let sector = 512usize;
        let mut buf = vec![0u8; sector * 256];
        buf[sector..sector + 8].copy_from_slice(GPT_SIGNATURE);
        buf[sector + 72..sector + 80].copy_from_slice(&2u64.to_le_bytes());
        buf[sector + 80..sector + 84].copy_from_slice(&4u32.to_le_bytes());
        buf[sector + 84..sector + 88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        let entries_off = sector * 2;
        buf[entries_off..entries_off + 16].copy_from_slice(&[1u8; 16]);
        buf[entries_off + 32..entries_off + 40].copy_from_slice(&100u64.to_le_bytes());
        buf[entries_off + 40..entries_off + 48].copy_from_slice(&199u64.to_le_bytes());
        let name = "data".encode_utf16().collect::<Vec<_>>();
        for (i, u) in name.iter().enumerate() {
            buf[entries_off + 56 + i * 2..entries_off + 58 + i * 2]
                .copy_from_slice(&u.to_le_bytes());
        }
        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].start_lba, 100);
        assert_eq!(parts[0].length_lba, 100);
        assert_eq!(parts[0].label.as_deref(), Some("data"));
    }

    /// P1-10: the manual walker must read the entry array from the source, not
    /// the 16-sector (0x2000) head probe. A 128-entry table at LBA 2 has its
    /// later entries well past byte 0x2000; an entry at index 100 (byte offset
    /// 0x2000 + …) must still be found.
    #[test]
    fn manual_fallback_reads_full_entry_array_past_head_probe() {
        let sector = 512usize;
        let mut buf = vec![0u8; sector * 512];
        buf[sector..sector + 8].copy_from_slice(GPT_SIGNATURE);
        buf[sector + 72..sector + 80].copy_from_slice(&2u64.to_le_bytes());
        buf[sector + 80..sector + 84].copy_from_slice(&128u32.to_le_bytes());
        buf[sector + 84..sector + 88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        // First entry keeps the leading-entry bounds check happy.
        let entries_off = sector * 2;
        buf[entries_off..entries_off + 16].copy_from_slice(&[1u8; 16]);
        buf[entries_off + 32..entries_off + 40].copy_from_slice(&34u64.to_le_bytes());
        buf[entries_off + 40..entries_off + 48].copy_from_slice(&99u64.to_le_bytes());
        // Entry index 100 lands at byte 2*512 + 100*128 = 0x3C00, past 0x2000.
        let far = entries_off + 100 * GPT_ENTRY_SIZE as usize;
        assert!(far > super::super::HEADER_PROBE_BYTES);
        buf[far..far + 16].copy_from_slice(&[2u8; 16]);
        buf[far + 32..far + 40].copy_from_slice(&200u64.to_le_bytes());
        buf[far + 40..far + 48].copy_from_slice(&299u64.to_le_bytes());
        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert_eq!(parts.len(), 2, "the entry past 0x2000 must be parsed");
        assert_eq!(parts[1].start_lba, 200);
    }

    /// P1-11: a lone `EFI PART` signature in a random sector with an entry that
    /// starts past the image end must be rejected, not minted into a garbage
    /// partition.
    #[test]
    fn rejects_manual_table_with_out_of_bounds_leading_entry() {
        let sector = 512usize;
        let mut buf = vec![0u8; sector * 64];
        buf[sector..sector + 8].copy_from_slice(GPT_SIGNATURE);
        buf[sector + 72..sector + 80].copy_from_slice(&2u64.to_le_bytes());
        buf[sector + 80..sector + 84].copy_from_slice(&4u32.to_le_bytes());
        buf[sector + 84..sector + 88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        let entries_off = sector * 2;
        buf[entries_off..entries_off + 16].copy_from_slice(&[1u8; 16]);
        // start_lba 9_000 is far past the 64-sector image → reject the table.
        buf[entries_off + 32..entries_off + 40].copy_from_slice(&9_000u64.to_le_bytes());
        buf[entries_off + 40..entries_off + 48].copy_from_slice(&9_100u64.to_le_bytes());
        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert!(
            parts.is_empty(),
            "out-of-bounds GPT must not yield partitions"
        );
    }

    /// P1-9: a wiped primary header with an intact backup header mirrored at the
    /// last LBA must still recover the table from the backup entry array.
    #[test]
    fn recovers_table_from_backup_header() {
        let sector = 512usize;
        let total = 256usize;
        let mut buf = vec![0u8; sector * total];
        // No primary `EFI PART` at LBA 1. Backup header sits at the last LBA.
        let backup_lba = total - 1;
        let backup_off = backup_lba * sector;
        buf[backup_off..backup_off + 8].copy_from_slice(GPT_SIGNATURE);
        // Backup entry array starts a few sectors before the backup header.
        let entries_lba = backup_lba - 32;
        buf[backup_off + 72..backup_off + 80].copy_from_slice(&(entries_lba as u64).to_le_bytes());
        buf[backup_off + 80..backup_off + 84].copy_from_slice(&4u32.to_le_bytes());
        buf[backup_off + 84..backup_off + 88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        let entries_off = entries_lba * sector;
        buf[entries_off..entries_off + 16].copy_from_slice(&[1u8; 16]);
        buf[entries_off + 32..entries_off + 40].copy_from_slice(&34u64.to_le_bytes());
        buf[entries_off + 40..entries_off + 48].copy_from_slice(&99u64.to_le_bytes());
        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert_eq!(parts.len(), 1, "backup header must recover the table");
        assert_eq!(parts[0].start_lba, 34);
        assert_eq!(parts[0].length_lba, 66);
    }

    /// Regression: an entry whose EndingLBA is u64::MAX must not overflow the
    /// `last_lba - first_lba + 1` length math (debug panic / release wrap).
    #[test]
    fn entry_with_max_ending_lba_does_not_overflow() {
        let sector = 512usize;
        let mut buf = vec![0u8; sector * 256];
        buf[sector..sector + 8].copy_from_slice(GPT_SIGNATURE);
        buf[sector + 72..sector + 80].copy_from_slice(&2u64.to_le_bytes());
        buf[sector + 80..sector + 84].copy_from_slice(&4u32.to_le_bytes());
        buf[sector + 84..sector + 88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        let entries_off = sector * 2;
        buf[entries_off..entries_off + 16].copy_from_slice(&[1u8; 16]);
        buf[entries_off + 32..entries_off + 40].copy_from_slice(&0u64.to_le_bytes());
        buf[entries_off + 40..entries_off + 48].copy_from_slice(&u64::MAX.to_le_bytes());
        let src = BufSource(buf);
        // Must not panic; the overflowing entry is skipped, and its huge start
        // is in bounds (0) but length overflows → dropped, leaving no parts.
        let parts = detect_partitions(&src, sector).expect("detect");
        assert!(parts.is_empty());
    }

    /// P1-14 + P1-9/10: a 4Kn image whose only `EFI PART` sits at LBA 7
    /// (byte 28672, past the 0x2000 head probe) must still be recovered via
    /// the source-backed signature probe.
    #[test]
    fn manual_fallback_finds_signature_past_head_probe_at_4kn() {
        let sector = 4096usize;
        let mut buf = vec![0u8; sector * 64];
        let header_off = 7 * sector;
        buf[header_off..header_off + 8].copy_from_slice(GPT_SIGNATURE);
        buf[header_off + 72..header_off + 80].copy_from_slice(&8u64.to_le_bytes());
        buf[header_off + 80..header_off + 84].copy_from_slice(&4u32.to_le_bytes());
        buf[header_off + 84..header_off + 88].copy_from_slice(&GPT_ENTRY_SIZE.to_le_bytes());
        let entries_off = 8 * sector;
        buf[entries_off..entries_off + 16].copy_from_slice(&[1u8; 16]);
        buf[entries_off + 32..entries_off + 40].copy_from_slice(&10u64.to_le_bytes());
        buf[entries_off + 40..entries_off + 48].copy_from_slice(&20u64.to_le_bytes());
        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].start_lba, 10);
    }

    #[test]
    fn maps_known_type_guids() {
        use gpt::partition_types as t;
        assert_eq!(guid_to_kind(t::LINUX_LVM.guid), PartitionKind::Lvm);
        assert_eq!(guid_to_kind(t::LINUX_SWAP.guid), PartitionKind::Swap);
        assert_eq!(guid_to_kind(t::BIOS.guid), PartitionKind::BiosBoot);
        assert_eq!(
            guid_to_kind(t::MACOS_APFS.guid),
            PartitionKind::ApfsContainer
        );
        assert_eq!(guid_to_kind(BITLOCKER_GUID), PartitionKind::Bitlocker);
        assert_eq!(guid_to_kind(FREEBSD_UFS_GUID), PartitionKind::Ufs);
        assert_eq!(guid_to_kind(FREEBSD_SWAP_GUID), PartitionKind::Swap);
        assert_eq!(guid_to_kind(FREEBSD_ZFS_GUID), PartitionKind::Zfs);
        assert_eq!(guid_to_kind(SOLARIS_ZFS_GUID), PartitionKind::Zfs);
        assert_eq!(guid_to_kind(MIDNIGHTBSD_ZFS_GUID), PartitionKind::Zfs);
        assert_eq!(guid_to_kind(t::BASIC.guid), PartitionKind::Unknown);
        assert_eq!(guid_to_kind(t::LINUX_FS.guid), PartitionKind::Unknown);
    }

    #[test]
    fn parse_entries_captures_unique_partition_guid() {
        // 单条 128 字节 GPT 条目：类型 GUID 在偏移 0，唯一 GUID 在偏移 16。
        let mut entry = vec![0u8; 128];
        entry[0..16].copy_from_slice(&[1u8; 16]); // 非零类型 GUID，否则条目被跳过
        let on_disk_unique = [
            0xDD, 0xCC, 0xBB, 0xAA, 0x22, 0x11, 0x44, 0x33, 0x55, 0x66, 0x77, 0x88, 0x99, 0x00,
            0x12, 0x34,
        ];
        entry[16..32].copy_from_slice(&on_disk_unique);
        entry[32..40].copy_from_slice(&100u64.to_le_bytes());
        entry[40..48].copy_from_slice(&199u64.to_le_bytes());
        let parts = parse_entries(&entry, 1, GPT_ENTRY_SIZE);
        assert_eq!(parts.len(), 1);
        let expected = uuid_from_disk_bytes(&on_disk_unique).unwrap();
        assert_eq!(parts[0].unique_guid, Some(*expected.as_bytes()));
    }

    #[test]
    fn uuid_from_disk_bytes_round_trips_through_uuid_string() {
        // BitLocker GUID on disk: data1/data2/data3 are LE, data4 is BE.
        let on_disk = [
            0x3B, 0xCA, 0xAF, 0xAE, 0xC1, 0x61, 0xE0, 0x40, 0x87, 0xA3, 0x47, 0xD1, 0xAF, 0x93,
            0xA9, 0xFC,
        ];
        let parsed = uuid_from_disk_bytes(&on_disk).expect("parse");
        assert_eq!(parsed, BITLOCKER_GUID);
    }
}
