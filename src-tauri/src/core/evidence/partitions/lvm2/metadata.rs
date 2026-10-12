//! LVM physical-volume metadata and first-segment byte geometry.
//! Shared by native volume browsing and cross-image RAID reconstruction.
use crate::core::evidence::image::{detect_filesystem, PartitionKind};
use crate::core::evidence::partitions::SectorSource;
use binrw::BinRead;
use std::io::Cursor;

#[derive(Debug, BinRead)]
#[br(little, magic = b"LABELONE")]
struct RawLvmLabelHeader {
    _sector_number: u64,
    _checksum: u32,
    data_offset: u32,
    type_tag: [u8; 8],
}

#[derive(Debug, BinRead)]
#[br(little)]
struct RawLvmDescriptor {
    offset: u64,
    size: u64,
}

impl RawLvmDescriptor {
    fn is_zero(&self) -> bool {
        self.offset == 0 && self.size == 0
    }
}

#[derive(Debug, BinRead)]
#[br(little)]
struct RawLvmMetadataAreaHeader {
    _checksum: u32,
    magic: [u8; 16],
    _version: u32,
    _metadata_area_offset: u64,
    metadata_area_size: u64,
}

#[derive(Debug, BinRead)]
#[br(little)]
struct RawLvmLocationDescriptor {
    data_area_offset: u64,
    data_area_size: u64,
    checksum: u32,
    flags: u32,
}

impl RawLvmLocationDescriptor {
    fn is_zero(&self) -> bool {
        self.data_area_offset == 0
            && self.data_area_size == 0
            && self.checksum == 0
            && self.flags == 0
    }
}

/// One logical volume's first-segment byte range plus the filesystem detected
/// at its start. Resolved directly from a `SectorSource`, so the RAID
/// metadata-less heuristic can descend into LVM on a candidate's assembled
/// device without first registering it as a `DiskImage`.
#[derive(Debug, Clone)]
pub struct LvRegion {
    pub name: String,
    /// Byte offset of the LV data area, relative to the LVM partition start
    /// (i.e. relative to `part_offset` passed to [`lv_regions_from_source`]).
    pub offset: u64,
    pub size_bytes: u64,
    pub kind: PartitionKind,
}

/// Enumerate the logical volumes of an LVM2 PV that begins at `part_offset`
/// within `source` (the PV occupies `[part_offset, part_offset + part_len)`).
/// Returns each LV's first-segment byte range and detected filesystem.
///
/// This is the `SectorSource` counterpart to [`LvmVolume::open`](crate::core::filesystem::volumes::lvm::LvmVolume::open): it shares the
/// same metadata parsing but resolves offsets against an arbitrary source
/// rather than a `DiskImage`, and returns lightweight ranges instead of the
/// navigable `LvmVolume`. Single-PV / first-segment only — sufficient for the
/// hardware-RAID reconstruction case (a single assembled device carrying one
/// PV), and never panics on a wrong reconstruction's garbage (every parse step
/// returns `None` and is skipped).
pub fn lv_regions_from_source(
    source: &(dyn SectorSource + Send + Sync),
    part_offset: u64,
    part_len: u64,
) -> Vec<LvRegion> {
    let probe_len = part_len.min(4 * 1024 * 1024) as usize;
    let Ok(probe) = source.read(part_offset, probe_len) else {
        return Vec::new();
    };
    let Some(metadata) = locate_active_metadata_region(&probe)
        .and_then(|(start, end)| probe.get(start..end).and_then(lvm_metadata_text))
        .or_else(|| lvm_metadata_text(&probe))
    else {
        return Vec::new();
    };
    let (Some(extent_size), Some(pe_start), Some(logical_block)) = (
        parse_assignment_u64(&metadata, "extent_size"),
        parse_assignment_u64(&metadata, "pe_start"),
        active_logical_volumes_block(&metadata),
    ) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (name, lv_block) in named_blocks(logical_block) {
        let Some((physical_extent, extent_count)) = parse_first_segment(&lv_block) else {
            continue;
        };
        // Sector size is fixed at 512 for an assembled RAID device.
        let geometry = physical_extent
            .checked_mul(extent_size)
            .and_then(|pe| pe.checked_add(pe_start))
            .and_then(|start_sector| {
                let sectors = extent_count.checked_mul(extent_size)?;
                let rel_offset = start_sector.checked_mul(512)?;
                let size_bytes = sectors.checked_mul(512)?;
                Some((rel_offset, size_bytes))
            });
        let Some((rel_offset, size_bytes)) = geometry else {
            continue;
        };
        let abs = match part_offset.checked_add(rel_offset) {
            Some(v) => v,
            None => continue,
        };
        let signature = match source.read(abs, 4096) {
            Ok(sig) if sig.len() == 4096 => sig,
            _ => continue,
        };
        let kind = detect_filesystem(&signature, PartitionKind::Unknown);
        out.push(LvRegion {
            name,
            offset: rel_offset,
            size_bytes,
            kind,
        });
    }
    out
}

/// Converts a raw byte slice to a sanitised text view: ASCII printable and
/// whitespace bytes pass through, everything else becomes `\n` so the
/// downstream brace/keyword scanners see a clean substrate.
pub(crate) fn sanitize_metadata_bytes_to_text(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte == b'\n' || byte == b'\r' || byte == b'\t' || (0x20..=0x7e).contains(&byte) {
            text.push(byte as char);
        } else {
            text.push('\n');
        }
    }
    text
}

/// Sanitises `bytes` and returns the text only when it actually looks like
/// LVM2 metadata (contains both `logical_volumes` and `physical_volumes`
/// keywords). Used both as the validation check on a binary-located active
/// region and as the entry point for the full-buffer text scan fallback.
pub(crate) fn lvm_metadata_text(bytes: &[u8]) -> Option<String> {
    let text = sanitize_metadata_bytes_to_text(bytes);
    if text.contains("logical_volumes") && text.contains("physical_volumes") {
        Some(text)
    } else {
        None
    }
}

/// Locates the active LVM2 metadata text region inside `probe` (the first
/// few MiB of a PV starting at partition offset 0).
///
/// LVM2's binary `mda_header` carries an authoritative pointer to the
/// currently active metadata text — far more precise than scanning the
/// rotating snapshot buffer for a maximum `seqno`. Returns `None` when any
/// of the binary headers fails to parse (custom MDA layout, truncated
/// probe, on-disk wrap, missing magic), in which case callers fall back to
/// the seqno heuristic against the full buffer.
///
/// The returned range is in bytes, relative to `probe`.
pub fn locate_active_metadata_region(probe: &[u8]) -> Option<(usize, usize)> {
    // PV label header lives in sector 1 (offset 512). Layout (32 bytes):
    //   "LABELONE"(8) + sector_number(u64) + checksum(u32)
    //   + data_offset(u32) + "LVM2 001"(8)
    const LABEL_HEADER_OFFSET: usize = 512;
    let label = probe.get(LABEL_HEADER_OFFSET..LABEL_HEADER_OFFSET + 32)?;
    let label = RawLvmLabelHeader::read(&mut Cursor::new(label)).ok()?;
    if label.type_tag != *b"LVM2 001" {
        return None;
    }
    let data_offset = label.data_offset as usize;

    // PhysicalVolumeHeader sits at LABEL_HEADER_OFFSET + data_offset:
    //   pv_ident(32) + pv_size(u64) + data_descriptors[] + metadata_descriptors[]
    // Each descriptor is offset(u64) + size(u64); each array is terminated
    // by a 16-byte zero entry.
    let pv_header_start = LABEL_HEADER_OFFSET.checked_add(data_offset)?;
    let descriptors_start = pv_header_start.checked_add(40)?;

    let mut cursor = descriptors_start;
    loop {
        let end = cursor.checked_add(16)?;
        let chunk = probe.get(cursor..end)?;
        let descriptor = RawLvmDescriptor::read(&mut Cursor::new(chunk)).ok()?;
        if descriptor.is_zero() {
            cursor = end;
            break;
        }
        cursor = end;
    }

    // First metadata descriptor — or the terminator, when no metadata.
    let md_chunk = probe.get(cursor..cursor.checked_add(16)?)?;
    let md_descriptor = RawLvmDescriptor::read(&mut Cursor::new(md_chunk)).ok()?;
    if md_descriptor.is_zero() {
        return None;
    }
    let mda_offset = usize::try_from(md_descriptor.offset).ok()?;
    if mda_offset == 0 {
        return None;
    }

    // MetadataAreaHeader at mda_offset:
    //   checksum(u32) + magic(16) + version(u32) + ma_offset(u64) + ma_size(u64)
    // Total fixed header = 40 bytes; locn_descriptors follow.
    let mda_header_end = mda_offset.checked_add(40)?;
    let mda_header = probe.get(mda_offset..mda_header_end)?;
    let mda_header = RawLvmMetadataAreaHeader::read(&mut Cursor::new(mda_header)).ok()?;
    if mda_header.magic != *b" LVM2 x[5A%r0N*>" {
        return None;
    }
    let metadata_area_size = usize::try_from(mda_header.metadata_area_size).ok()?;

    // First location_descriptor: data_area_offset(u64) + data_area_size(u64)
    // + checksum(u32) + flags(u32). All-zero means no active locn.
    let locn = probe.get(mda_header_end..mda_header_end.checked_add(24)?)?;
    let locn = RawLvmLocationDescriptor::read(&mut Cursor::new(locn)).ok()?;
    if locn.is_zero() {
        return None;
    }
    let data_area_offset = usize::try_from(locn.data_area_offset).ok()?;
    let data_area_size = usize::try_from(locn.data_area_size).ok()?;
    if data_area_size == 0 {
        return None;
    }

    // When the active text wraps the rotating buffer, it isn't a single
    // contiguous range — caller falls back to the seqno-based text scan.
    if data_area_offset.checked_add(data_area_size)? > metadata_area_size {
        return None;
    }

    let abs_start = mda_offset.checked_add(data_area_offset)?;
    let abs_end = abs_start.checked_add(data_area_size)?;
    if abs_end > probe.len() {
        return None;
    }
    Some((abs_start, abs_end))
}

pub(crate) fn parse_assignment_u64(text: &str, key: &str) -> Option<u64> {
    let pos = text.find(key)?;
    let after_key = &text[pos + key.len()..];
    let eq = after_key.find('=')?;
    let value = after_key[eq + 1..].trim_start();
    let digits = value
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

fn extract_block(text: &str, key: &str) -> Option<String> {
    let key_pos = text.find(key)?;
    let open_rel = text[key_pos..].find('{')?;
    let open = key_pos + open_rel;
    let close = matching_brace(text, open)?;
    Some(text[open + 1..close].to_string())
}

/// LVM2 keeps every metadata revision in the PV's mda area — each
/// `lvcreate` / `lvextend` appends a fresh `<vg> { ... }` block with an
/// incremented `seqno`. The active state is the snapshot with the
/// highest `seqno`; the first one in byte order is the *oldest* (the
/// one written by `vgcreate`), so a naive `extract_block(text,
/// "logical_volumes")` would silently drop every LV created after VG
/// initialisation. Mirrors the same fix in `raid::lvm_meta::top_level_vg`.
pub(crate) fn active_logical_volumes_block(text: &str) -> Option<String> {
    let mut best: Option<(u64, String)> = None;
    for (_name, block) in named_blocks(text.to_string()) {
        if !block.contains("physical_volumes") || !block.contains("logical_volumes") {
            continue;
        }
        let seqno = parse_assignment_u64(&block, "seqno").unwrap_or(0);
        let lvs = extract_block(&block, "logical_volumes")?;
        match &best {
            Some((b_seqno, _)) if *b_seqno >= seqno => {}
            _ => best = Some((seqno, lvs)),
        }
    }
    best.map(|(_, lvs)| lvs)
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (idx, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + idx);
                }
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn named_blocks(text: String) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let bytes = text.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || bytes[cursor] == b'_'
                || bytes[cursor] == b'-'
                || bytes[cursor] == b'.')
        {
            cursor += 1;
        }
        if cursor == name_start {
            cursor += 1;
            continue;
        }
        let name = text[name_start..cursor].to_string();
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'{' {
            continue;
        }
        if let Some(close) = matching_brace(&text, cursor) {
            out.push((name, text[cursor + 1..close].to_string()));
            cursor = close + 1;
        } else {
            break;
        }
    }
    out
}

pub(crate) fn parse_first_segment(lv_block: &str) -> Option<(u64, u64)> {
    let segment = named_blocks(lv_block.to_string())
        .into_iter()
        .find(|(name, _)| name.starts_with("segment"))?
        .1;
    let extent_count = parse_assignment_u64(&segment, "extent_count")?;
    let stripes_pos = segment.find("stripes")?;
    let stripes = &segment[stripes_pos..];
    let open = stripes.find('[')?;
    let close = stripes[open..].find(']')? + open;
    let inside = &stripes[open + 1..close];
    let comma = inside.rfind(',')?;
    let physical_extent = inside[comma + 1..]
        .trim()
        .trim_matches('"')
        .parse::<u64>()
        .ok()?;
    Some((physical_extent, extent_count))
}

#[cfg(test)]
mod tests;
