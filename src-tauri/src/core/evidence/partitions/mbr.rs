//! MBR + extended-boot-record detection.
//!
//! Walks the four primary entries at offset 0x1BE, then chains through any
//! extended partition's EBR list to surface logical partitions. Uses an
//! explicit visited-set guard and a follow
//! cap so cyclic EBR chains terminate.

use anyhow::Context;
use binrw::{BinRead, BinReaderExt};
use std::io::Cursor;

use crate::core::evidence::partitions::{
    volume_probe::probe_volume, DetectCtx, PartParser, Partition, PartitionKind, PartitionSlice,
    SectorSource, TableScheme,
};

const MBR_MAGIC: [u8; 2] = [0x55, 0xAA];
const MBR_PART_TABLE_OFFSET: usize = 0x1BE;
const MBR_PART_ENTRY_SIZE: usize = 16;
const MBR_ENTRY_COUNT: usize = 4;
const MAX_EBR_FOLLOW: usize = 64;
/// NT disk signature lives at bytes 440..444 of the boot sector.
const MBR_DISK_SIGNATURE_OFFSET: usize = 440;

/// Read the 32-bit NT disk signature out of the boot sector. Best effort —
/// `None` when the sector is short or the signature was never stamped (zero).
pub(crate) fn read_disk_signature(head: &[u8]) -> Option<String> {
    let bytes = head.get(MBR_DISK_SIGNATURE_OFFSET..MBR_DISK_SIGNATURE_OFFSET + 4)?;
    let signature = Cursor::new(bytes).read_le::<u32>().ok()?;
    (signature != 0).then(|| format!("0x{signature:08X}"))
}

pub struct MbrParser;

#[derive(Debug, Clone, Copy)]
struct MbrEntry {
    type_byte: u8,
    start_lba: u64,
    length_lba: u64,
}

#[derive(Debug, BinRead)]
#[br(little)]
struct RawMbrEntry {
    _status: u8,
    _first_chs: [u8; 3],
    type_byte: u8,
    _last_chs: [u8; 3],
    start_lba: u32,
    length_lba: u32,
}

impl MbrEntry {
    fn is_extended(&self) -> bool {
        matches!(self.type_byte, 0x05 | 0x0F | 0x85)
    }
}

impl PartParser for MbrParser {
    fn name(&self) -> &'static str {
        "mbr"
    }

    fn scheme(&self) -> TableScheme {
        TableScheme::Mbr
    }

    fn try_parse(&self, ctx: &DetectCtx<'_>) -> anyhow::Result<Option<Vec<Partition>>> {
        let head = ctx.head;
        if head.len() < 512 {
            return Ok(None);
        }
        // Volume boot sectors also end in 0x55AA. Their boot code and text
        // overlap the primary table, and on large images even those bogus
        // LBAs can fall in bounds. Every table slot must have a legal boot
        // indicator before interpreting any bytes as partitions.
        if head[MBR_PART_TABLE_OFFSET..510]
            .as_chunks::<MBR_PART_ENTRY_SIZE>()
            .0
            .iter()
            .any(|entry| !matches!(entry[0], 0x00 | 0x80))
        {
            return Ok(None);
        }
        let has_magic = head[510..512] == MBR_MAGIC;
        if !has_magic && !plausible_unsigned_mbr(ctx)? {
            return Ok(None);
        }
        if !has_magic {
            tracing::warn!(
                "recovering MBR partition table with missing 0x55AA signature after structural and volume-signature validation"
            );
        }
        // Largest addressable sector. A real primary table never starts a
        // partition at or past this; a random sector that merely ends in
        // 0x55AA does (P1-11). `0` means we cannot judge, so skip the gate.
        let max_sector = (ctx.sector_size != 0).then(|| ctx.size / ctx.sector_size as u64);

        let mut out = Vec::with_capacity(MBR_ENTRY_COUNT);
        let mut next_id: u8 = 0;
        let mut extended_base: Option<u64> = None;

        for index in 0..MBR_ENTRY_COUNT {
            let base = MBR_PART_TABLE_OFFSET + index * MBR_PART_ENTRY_SIZE;
            let entry = &head[base..base + MBR_PART_ENTRY_SIZE];
            let Some(parsed) = parse_mbr_entry(entry) else {
                continue;
            };
            // Leading-entry bounds sanity: if either of the first two primary entries
            // starts past the image, reject the whole table so the dispatcher
            // moves on rather than minting garbage from a stray boot sector.
            if let Some(max_sector) = max_sector {
                if index < 2 && parsed.start_lba >= max_sector {
                    return Ok(None);
                }
            }
            if parsed.is_extended() {
                if extended_base.is_none() {
                    extended_base = Some(parsed.start_lba);
                }
                continue;
            }
            // Drop trailing entries (index 2/3) that start past the image; the
            // leading ones already rejected the table above.
            if max_sector.is_some_and(|max| parsed.start_lba >= max) {
                continue;
            }
            out.push(Partition {
                id: next_id,
                kind: mbr_type_to_kind(parsed.type_byte),
                start_lba: parsed.start_lba,
                length_lba: parsed.length_lba,
                label: None,
                unique_guid: None,
            });
            next_id = next_id.wrapping_add(1);
        }

        if let Some(extended_base) = extended_base {
            follow_ebr_chain(
                ctx.source,
                ctx.sector_size,
                extended_base,
                max_sector,
                &mut out,
                &mut next_id,
            )?;
        }
        if out.is_empty() {
            return Ok(None);
        }
        Ok(Some(out))
    }
}

/// Recover a damaged MBR whose final `0x55AA` marker was erased. Without the
/// marker, table-shaped bytes in an ordinary boot sector are too weak to trust,
/// so every populated entry must be structurally valid and at least one normal
/// partition must independently match a supported volume signature.
fn plausible_unsigned_mbr(ctx: &DetectCtx<'_>) -> anyhow::Result<bool> {
    if ctx.sector_size == 0 {
        return Ok(false);
    }
    let max_sector = ctx.size / ctx.sector_size as u64;
    let mut populated = 0usize;
    let mut active = 0usize;
    let mut confirmed_volume = false;
    let mut ranges = Vec::with_capacity(MBR_ENTRY_COUNT);

    for index in 0..MBR_ENTRY_COUNT {
        let base = MBR_PART_TABLE_OFFSET + index * MBR_PART_ENTRY_SIZE;
        let entry = &ctx.head[base..base + MBR_PART_ENTRY_SIZE];
        let status = entry[0];
        if !matches!(status, 0x00 | 0x80) {
            return Ok(false);
        }
        active += usize::from(status == 0x80);

        let type_byte = entry[4];
        let start_lba = u64::from(u32::from_le_bytes(entry[8..12].try_into()?));
        let length_lba = u64::from(u32::from_le_bytes(entry[12..16].try_into()?));
        if type_byte == 0 {
            if entry.iter().any(|byte| *byte != 0) {
                return Ok(false);
            }
            continue;
        }
        let Some(end_lba) = start_lba.checked_add(length_lba) else {
            return Ok(false);
        };
        if start_lba == 0 || length_lba == 0 || end_lba > max_sector {
            return Ok(false);
        }

        populated += 1;
        ranges.push((start_lba, end_lba));
        if !(MbrEntry {
            type_byte,
            start_lba,
            length_lba,
        })
        .is_extended()
        {
            let slice = PartitionSlice::new(
                ctx.source,
                start_lba * ctx.sector_size as u64,
                length_lba * ctx.sector_size as u64,
            );
            confirmed_volume |= probe_volume(&slice, ctx.sector_size)?.is_some();
        }
    }

    if populated == 0 || active > 1 || !confirmed_volume {
        return Ok(false);
    }
    ranges.sort_unstable_by_key(|range| range.0);
    Ok(ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0))
}

fn parse_mbr_entry(entry: &[u8]) -> Option<MbrEntry> {
    let raw = RawMbrEntry::read(&mut Cursor::new(entry)).ok()?;
    if raw.type_byte == 0 {
        return None;
    }
    let start_lba = u64::from(raw.start_lba);
    let length_lba = u64::from(raw.length_lba);
    if length_lba == 0 {
        return None;
    }
    Some(MbrEntry {
        type_byte: raw.type_byte,
        start_lba,
        length_lba,
    })
}

fn follow_ebr_chain(
    source: &dyn SectorSource,
    sector_size: usize,
    extended_base: u64,
    max_sector: Option<u64>,
    out: &mut Vec<Partition>,
    next_id: &mut u8,
) -> anyhow::Result<()> {
    let mut current = extended_base;
    let mut visited: Vec<u64> = Vec::with_capacity(8);
    for _ in 0..MAX_EBR_FOLLOW {
        if visited.contains(&current) {
            break;
        }
        visited.push(current);

        let sector_offset = current
            .checked_mul(sector_size as u64)
            .context("EBR offset overflow")?;
        let sector = source.read(sector_offset, sector_size)?;
        if sector.len() < 512 || sector[510..512] != MBR_MAGIC {
            break;
        }

        // Walk all four entries in the EBR rather than only the first data +
        // first link slot (P1-12): some partitioning tools place a second
        // logical partition in slot 2/3, and those were being silently
        // dropped. Data entries are addressed relative to this EBR sector;
        // the link (extended) entry is relative to the primary extended base.
        let mut next: Option<u64> = None;
        for index in 0..MBR_ENTRY_COUNT {
            let base = MBR_PART_TABLE_OFFSET + index * MBR_PART_ENTRY_SIZE;
            let entry = &sector[base..base + MBR_PART_ENTRY_SIZE];
            let Some(parsed) = parse_mbr_entry(entry) else {
                continue;
            };
            if parsed.is_extended() {
                // First extended link wins; chasing more than one per table
                // would fork the iterative chain traversal.
                if next.is_none() && parsed.start_lba != 0 {
                    next = Some(extended_base + parsed.start_lba);
                }
                continue;
            }
            let start_lba = current + parsed.start_lba;
            // A partition starting past the image is unreadable noise; drop it.
            if max_sector.is_some_and(|max| start_lba >= max) {
                continue;
            }
            out.push(Partition {
                id: *next_id,
                kind: mbr_type_to_kind(parsed.type_byte),
                start_lba,
                length_lba: parsed.length_lba,
                label: None,
                unique_guid: None,
            });
            *next_id = next_id.wrapping_add(1);
        }

        let Some(next) = next else {
            break;
        };
        current = next;
    }
    Ok(())
}

fn mbr_type_to_kind(type_byte: u8) -> PartitionKind {
    match type_byte {
        0x07 => PartitionKind::Ntfs,
        0x0B | 0x0C => PartitionKind::Fat32,
        0x04 | 0x06 | 0x0E => PartitionKind::Fat16,
        0x05 | 0x0F | 0x85 => PartitionKind::Extended,
        0x82 => PartitionKind::Swap,
        0x83 => PartitionKind::Unknown,
        0x8E => PartitionKind::Lvm,
        0xA5 | 0xA6 | 0xA9 => PartitionKind::Bsd,
        0xAF => PartitionKind::ApfsContainer,
        _ => PartitionKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::evidence::partitions::{
        detect_partitions, detect_partitions_with_meta, test_support::BufSource, HEADER_PROBE_BYTES,
    };

    #[test]
    fn detects_large_ntfs_volume_with_boot_messages_as_volume() {
        // Logical acquisitions can retain the source disk's 2 TB size while
        // beginning directly with a smaller NTFS volume. These actual Windows
        // boot messages form four nonzero, in-bounds pseudo-partitions.
        struct LargeVolumeSource(BufSource);
        impl SectorSource for LargeVolumeSource {
            fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
                self.0.read(offset, len)
            }

            fn size(&self) -> u64 {
                3_907_024_065 * 512
            }
        }

        let mut head = vec![0u8; HEADER_PROBE_BYTES];
        head[..11].copy_from_slice(b"\xeb\x52\x90NTFS    ");
        head[11..13].copy_from_slice(&512u16.to_le_bytes());
        head[13] = 8;
        head[40..48].copy_from_slice(&537_894_911u64.to_le_bytes());
        head[48..56].copy_from_slice(&786_432u64.to_le_bytes());
        head[MBR_PART_TABLE_OFFSET..510].copy_from_slice(
            b"\r\nBOOTMGR is compressed\0\r\nPress Ctrl+Alt+Del to restart\r\n\0\x8c\xa9\xbe\xd6\0\0",
        );
        head[510..512].copy_from_slice(&MBR_MAGIC);
        let source = LargeVolumeSource(BufSource(head));
        let detected = detect_partitions_with_meta(&source, 512).expect("detect NTFS volume");

        assert_eq!(
            (detected.scheme, detected.disk_id, detected.partitions),
            (
                Some(TableScheme::Volume),
                None,
                vec![Partition {
                    id: 0,
                    kind: PartitionKind::Ntfs,
                    start_lba: 0,
                    length_lba: 3_907_024_065,
                    label: None,
                    unique_guid: None,
                }],
            )
        );
    }

    #[test]
    fn rejects_signed_table_with_invalid_boot_indicator_in_any_slot() {
        for invalid_slot in 0..MBR_ENTRY_COUNT {
            let mut head = vec![0u8; 512 * 64];
            head[510..512].copy_from_slice(&MBR_MAGIC);
            for slot in 0..MBR_ENTRY_COUNT {
                let base = MBR_PART_TABLE_OFFSET + slot * MBR_PART_ENTRY_SIZE;
                head[base] = if slot == invalid_slot { 0x7f } else { 0x00 };
                head[base + 4] = 0x07;
                head[base + 8..base + 12].copy_from_slice(&(1 + slot as u32 * 8).to_le_bytes());
                head[base + 12..base + 16].copy_from_slice(&8u32.to_le_bytes());
            }
            let source = BufSource(head);
            let ctx = DetectCtx {
                source: &source,
                head: &source.0,
                sector_size: 512,
                size: source.size(),
            };
            assert_eq!(MbrParser.try_parse(&ctx).expect("parse MBR"), None);
        }
    }

    #[test]
    fn parses_mbr_with_primary_partitions() {
        // Image large enough that LBA 2048 + 10 stays in bounds (P1-11).
        let mut buf = vec![0u8; 512 * 2100];
        buf[510] = 0x55;
        buf[511] = 0xAA;
        let e0 = MBR_PART_TABLE_OFFSET;
        buf[e0] = 0x80;
        buf[e0 + 4] = 0x07;
        buf[e0 + 8..e0 + 12].copy_from_slice(&2048u32.to_le_bytes());
        buf[e0 + 12..e0 + 16].copy_from_slice(&10u32.to_le_bytes());
        let src = BufSource(buf);
        let parts = detect_partitions(&src, 512).expect("detect");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, PartitionKind::Ntfs);
        assert_eq!(parts[0].start_lba, 2048);
    }

    #[test]
    fn follows_ebr_chain() {
        let sector = 512usize;
        let ext_base_lba = 2048u32;
        let mut buf = vec![0u8; sector * (ext_base_lba as usize + 200)];
        buf[510] = 0x55;
        buf[511] = 0xAA;
        let e0 = MBR_PART_TABLE_OFFSET;
        buf[e0 + 4] = 0x05;
        buf[e0 + 8..e0 + 12].copy_from_slice(&ext_base_lba.to_le_bytes());
        buf[e0 + 12..e0 + 16].copy_from_slice(&200u32.to_le_bytes());

        let ebr1 = ext_base_lba as usize * sector;
        buf[ebr1 + 510] = 0x55;
        buf[ebr1 + 511] = 0xAA;
        buf[ebr1 + e0 + 4] = 0x83;
        buf[ebr1 + e0 + 8..ebr1 + e0 + 12].copy_from_slice(&1u32.to_le_bytes());
        buf[ebr1 + e0 + 12..ebr1 + e0 + 16].copy_from_slice(&50u32.to_le_bytes());

        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].start_lba, ext_base_lba as u64 + 1);
        assert_eq!(parts[0].length_lba, 50);
    }

    #[test]
    fn ignores_boot_sector_without_partition_entries() {
        let mut buf = vec![0u8; 4096];
        buf[510] = 0x55;
        buf[511] = 0xAA;
        let src = BufSource(buf);

        let parts = detect_partitions(&src, 512).expect("detect");

        assert!(parts.is_empty());
    }

    /// P1-12: a logical partition placed in EBR slot 2 (not the conventional
    /// data slot 0) must still be surfaced, alongside the slot-0 data entry.
    #[test]
    fn follows_ebr_chain_with_data_entry_in_slot_two() {
        let sector = 512usize;
        let ext_base_lba = 2048usize;
        let mut buf = vec![0u8; sector * (ext_base_lba + 400)];
        buf[510] = 0x55;
        buf[511] = 0xAA;
        let e0 = MBR_PART_TABLE_OFFSET;
        buf[e0 + 4] = 0x05;
        buf[e0 + 8..e0 + 12].copy_from_slice(&(ext_base_lba as u32).to_le_bytes());
        buf[e0 + 12..e0 + 16].copy_from_slice(&400u32.to_le_bytes());

        let ebr1 = ext_base_lba * sector;
        buf[ebr1 + 510] = 0x55;
        buf[ebr1 + 511] = 0xAA;
        // Slot 0: a normal logical FAT16 partition.
        buf[ebr1 + e0 + 4] = 0x06;
        buf[ebr1 + e0 + 8..ebr1 + e0 + 12].copy_from_slice(&1u32.to_le_bytes());
        buf[ebr1 + e0 + 12..ebr1 + e0 + 16].copy_from_slice(&50u32.to_le_bytes());
        // Slot 2: a second logical partition that the old single-slot reader
        // dropped. Entry stride is MBR_PART_ENTRY_SIZE (16 bytes).
        let s2 = e0 + 2 * MBR_PART_ENTRY_SIZE;
        buf[ebr1 + s2 + 4] = 0x83;
        buf[ebr1 + s2 + 8..ebr1 + s2 + 12].copy_from_slice(&60u32.to_le_bytes());
        buf[ebr1 + s2 + 12..ebr1 + s2 + 16].copy_from_slice(&70u32.to_le_bytes());

        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert_eq!(parts.len(), 2, "both logical entries must be surfaced");
        assert_eq!(parts[0].start_lba, ext_base_lba as u64 + 1);
        assert_eq!(parts[0].kind, PartitionKind::Fat16);
        assert_eq!(parts[1].start_lba, ext_base_lba as u64 + 60);
    }

    /// P1-11: a stray sector ending in 0x55AA whose leading entry starts past
    /// the image must not be accepted as a partition table.
    #[test]
    fn rejects_primary_table_with_out_of_bounds_leading_entry() {
        let sector = 512usize;
        let mut buf = vec![0u8; sector * 64];
        buf[510] = 0x55;
        buf[511] = 0xAA;
        let e0 = MBR_PART_TABLE_OFFSET;
        buf[e0 + 4] = 0x07;
        // start_lba 9_000 is well past the 64-sector image.
        buf[e0 + 8..e0 + 12].copy_from_slice(&9_000u32.to_le_bytes());
        buf[e0 + 12..e0 + 16].copy_from_slice(&10u32.to_le_bytes());
        let src = BufSource(buf);
        let parts = detect_partitions(&src, sector).expect("detect");
        assert!(
            parts.is_empty(),
            "out-of-bounds MBR must not yield partitions"
        );
    }

    #[test]
    fn recovers_unsigned_mbr_when_partition_signature_confirms_it() {
        let sector = 512usize;
        let start_lba = 2048u32;
        let length_lba = 2048u32;
        let mut buf = vec![0u8; sector * 4096];
        let entry = MBR_PART_TABLE_OFFSET;
        buf[entry] = 0x80;
        buf[entry + 4] = 0x83;
        buf[entry + 8..entry + 12].copy_from_slice(&start_lba.to_le_bytes());
        buf[entry + 12..entry + 16].copy_from_slice(&length_lba.to_le_bytes());
        let ext_magic = start_lba as usize * sector + 0x438;
        buf[ext_magic..ext_magic + 2].copy_from_slice(&[0x53, 0xEF]);

        let parts = detect_partitions(&BufSource(buf), sector).expect("detect");

        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].start_lba, u64::from(start_lba));
        assert_eq!(parts[0].length_lba, u64::from(length_lba));
    }

    #[test]
    fn rejects_unsigned_table_without_confirmed_partition_signature() {
        let sector = 512usize;
        let mut buf = vec![0u8; sector * 4096];
        let entry = MBR_PART_TABLE_OFFSET;
        buf[entry] = 0x80;
        buf[entry + 4] = 0x83;
        buf[entry + 8..entry + 12].copy_from_slice(&2048u32.to_le_bytes());
        buf[entry + 12..entry + 16].copy_from_slice(&2048u32.to_le_bytes());

        let parts = detect_partitions(&BufSource(buf), sector).expect("detect");

        assert!(parts.is_empty());
    }
}
