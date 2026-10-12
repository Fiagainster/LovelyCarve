//! Full LVM2 text-metadata parsing for cross-image VG/LV reconstruction.
//!
//! `lvm_volume.rs` only resolves the **first** segment of each LV against a
//! single PV — fine for an LV that lives entirely on one disk, useless for
//! an LV whose segments fan out across PVs that themselves live on
//! different physical images. The `检材3` case is the latter: VG `volum`
//! has its PVs split between `检材3-1.E01` and `检材3-2.E01`, and each LV
//! has two `striped`/`stripe_count=1` segments — i.e. linear concatenation
//! across the two physical disks.
//!
//! This module parses the full PV-header text-metadata block emitted by
//! `vgcreate`/`lvcreate` and extracts:
//!
//! - VG UUID + name + extent_size (sectors)
//! - PV UUID + pe_start (sectors) + size
//! - LV name + ordered list of segments
//!   - Each segment: `(pv_uuid, pv_extent_offset, extent_count)` for the
//!     `stripe_count=1` "linear-as-striped" form documented in the case
//!     report
//!
//! Cross-image grouping (joining PVs by VG UUID) and LV materialisation
//! (turning LV-relative offsets into per-image byte ranges) live in
//! `raid_assembly.rs` so this module stays focused on text parsing.

use std::collections::BTreeMap;

use anyhow::Context;

use crate::core::evidence::image::DiskImage;
use crate::core::evidence::partitions::lvm2::metadata::{
    locate_active_metadata_region, lvm_metadata_text,
};
use crate::core::evidence::partitions::{Partition, PartitionKind};

/// One PV's parsed metadata block, scoped to the image it was read from so
/// `raid_assembly.rs` can map LV-segment references back to absolute byte
/// offsets without a second pass over the disk.
#[derive(Debug, Clone)]
pub struct LvmPvMetadata {
    pub image_id: String,
    pub partition_id: u8,
    pub partition_byte_offset: u64,
    pub partition_byte_length: u64,
    pub sector_size: usize,
    /// 32-char hyphenated UUID exactly as it appears in the LVM text block.
    pub vg_uuid: String,
    pub vg_name: String,
    /// PV identifier used inside the LV `stripes = [...]` array (e.g. `"pv0"`,
    /// `"pv1"`).
    pub pv_local_id: String,
    pub pv_uuid: String,
    /// `extent_size` in 512-byte sectors. Default is 8192 sectors (4 MiB).
    pub extent_size_sectors: u64,
    /// `pe_start` in 512-byte sectors. Default is 2048 sectors (1 MiB).
    pub pe_start_sectors: u64,
    /// Logical volumes defined by this VG. Identical across PVs of the same
    /// VG — duplicated here so each PV record is self-contained.
    pub logical_volumes: Vec<LvmLogicalVolumeMeta>,
}

#[derive(Debug, Clone)]
pub struct LvmLogicalVolumeMeta {
    pub name: String,
    pub segments: Vec<LvmSegment>,
}

#[derive(Debug, Clone)]
pub struct LvmSegment {
    /// PV identifier of the form `"pv0"` / `"pv1"` — resolves against the
    /// `pv_local_id` field of `LvmPvMetadata`.
    pub pv_local_id: String,
    /// Starting extent index inside that PV.
    pub pv_extent_offset: u64,
    /// Number of extents this segment covers.
    pub extent_count: u64,
    /// `stripe_count` from the LVM segment block. `1` is linear-on-one-PV
    /// even though the type is reported as `striped` (LVM's own quirk —
    /// "single-stripe striped" is just linear).
    pub stripe_count: u64,
}

/// Parse a single LVM2 PV partition's textual metadata into the structured
/// record used by `raid_assembly.rs`. Returns `Ok(None)` when the partition
/// isn't an LVM PV or the metadata block is missing — callers can keep
/// scanning other partitions without bubbling that as an error.
pub fn read_pv_metadata(
    image: &DiskImage,
    partition: &Partition,
) -> anyhow::Result<Option<LvmPvMetadata>> {
    if partition.kind != PartitionKind::Lvm {
        return Ok(None);
    }
    let partition_byte_offset = partition.start_lba * image.sector_size as u64;
    let partition_byte_length = partition.length_lba * image.sector_size as u64;
    // 4 MiB is enough to cover the PV label + header + the metadata text
    // area — matches what `lvm_volume.rs` already uses for single-segment
    // LV resolution.
    let probe_len = partition_byte_length.min(4 * 1024 * 1024) as usize;
    let probe = image
        .read_range(partition_byte_offset, probe_len)
        .context("reading PV metadata probe")?;
    // Prefer the binary mda_header pointer to the active text region; fall
    // back to a full-buffer text scan when the binary layout doesn't parse.
    // The fallback path's `top_level_vg` then picks the highest-`seqno`
    // snapshot to avoid the "first match in byte order is the oldest"
    // failure mode.
    let Some(metadata) = locate_active_metadata_region(&probe)
        .and_then(|(start, end)| lvm_metadata_text(&probe[start..end]))
        .or_else(|| lvm_metadata_text(&probe))
    else {
        return Ok(None);
    };

    let extent_size_sectors = parse_assignment_u64(&metadata, "extent_size").unwrap_or(8192);
    let pe_start_sectors = parse_assignment_u64(&metadata, "pe_start").unwrap_or(2048);

    let (vg_name, vg_block) = match top_level_vg(&metadata) {
        Some(pair) => pair,
        None => return Ok(None),
    };
    let vg_uuid = parse_assignment_string(&vg_block, "id").unwrap_or_default();

    let pvs_block = extract_block(&vg_block, "physical_volumes").unwrap_or_default();
    let (pv_local_id, pv_uuid) = pick_pv_for_partition(&pvs_block, partition_byte_length)
        .unwrap_or_else(|| ("pv0".into(), String::new()));

    let logical_volumes = extract_block(&vg_block, "logical_volumes")
        .map(|block| parse_logical_volumes(&block))
        .unwrap_or_default();

    Ok(Some(LvmPvMetadata {
        image_id: image.id.clone(),
        partition_id: partition.id,
        partition_byte_offset,
        partition_byte_length,
        sector_size: image.sector_size,
        vg_uuid,
        vg_name,
        pv_local_id,
        pv_uuid,
        extent_size_sectors,
        pe_start_sectors,
        logical_volumes,
    }))
}

fn parse_assignment_u64(text: &str, key: &str) -> Option<u64> {
    let value = parse_assignment_raw(text, key)?;
    let digits = value
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

fn parse_assignment_string(text: &str, key: &str) -> Option<String> {
    let value = parse_assignment_raw(text, key)?;
    let trimmed = value.trim();
    if let Some(stripped) = trimmed.strip_prefix('"') {
        if let Some(end) = stripped.find('"') {
            return Some(stripped[..end].to_string());
        }
    }
    Some(trimmed.split_whitespace().next()?.to_string())
}

fn parse_assignment_raw<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut cursor = 0;
    while cursor < text.len() {
        let pos = text[cursor..].find(key)?;
        let absolute = cursor + pos;
        let prev = absolute
            .checked_sub(1)
            .and_then(|idx| text.as_bytes().get(idx).copied());
        let next = text.as_bytes().get(absolute + key.len()).copied();
        // Ensure key is not embedded inside another identifier. Examples like
        // `pe_start` vs `extent_pe_start` would otherwise match the wrong key.
        let prev_ok = prev.is_none_or(|b| !is_word_char(b));
        let next_ok = next.is_none_or(|b| !is_word_char(b));
        if prev_ok && next_ok {
            let after = &text[absolute + key.len()..];
            let eq = after.find('=')?;
            return Some(after[eq + 1..].trim_start());
        }
        cursor = absolute + key.len();
    }
    None
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
}

fn extract_block(text: &str, key: &str) -> Option<String> {
    let key_pos = find_top_level_key(text, key)?;
    let open_rel = text[key_pos..].find('{')?;
    let open = key_pos + open_rel;
    let close = matching_brace(text, open)?;
    Some(text[open + 1..close].to_string())
}

fn find_top_level_key(text: &str, key: &str) -> Option<usize> {
    let mut cursor = 0;
    while cursor < text.len() {
        let pos = text[cursor..].find(key)?;
        let absolute = cursor + pos;
        let prev = absolute
            .checked_sub(1)
            .and_then(|idx| text.as_bytes().get(idx).copied());
        let next = text.as_bytes().get(absolute + key.len()).copied();
        let prev_ok = prev.is_none_or(|b| !is_word_char(b));
        let next_ok = next.is_none_or(|b| !is_word_char(b));
        if prev_ok && next_ok {
            return Some(absolute);
        }
        cursor = absolute + key.len();
    }
    None
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

fn named_blocks(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let bytes = text.as_bytes();
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < bytes.len() && is_word_char(bytes[cursor]) {
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
            // Skip past assignments/strings until we reach a brace or
            // end-of-buffer; otherwise we'd interpret every word as a block
            // name and produce noise.
            while cursor < bytes.len() && bytes[cursor] != b'{' && bytes[cursor] != b'\n' {
                cursor += 1;
            }
            continue;
        }
        if let Some(close) = matching_brace(text, cursor) {
            out.push((name, text[cursor + 1..close].to_string()));
            cursor = close + 1;
        } else {
            break;
        }
    }
    out
}

fn top_level_vg(text: &str) -> Option<(String, String)> {
    // The PV's metadata area holds every revision LVM has written —
    // `vgcreate`, every `lvcreate`/`lvextend`, etc. each append a fresh VG
    // block with an incremented `seqno`. The active state is the one with
    // the highest `seqno`; returning the first match (oldest snapshot)
    // makes us miss every LV created after VG initialisation.
    let mut best: Option<(u64, String, String)> = None;
    for (name, block) in named_blocks(text) {
        if !block.contains("physical_volumes") || !block.contains("logical_volumes") {
            continue;
        }
        let seqno = parse_assignment_u64(&block, "seqno").unwrap_or(0);
        match &best {
            Some((b_seqno, _, _)) if *b_seqno >= seqno => {}
            _ => best = Some((seqno, name, block)),
        }
    }
    best.map(|(_, name, block)| (name, block))
}

fn pick_pv_for_partition(pvs_block: &str, partition_bytes: u64) -> Option<(String, String)> {
    // The metadata block contains every PV that belongs to the VG, but the
    // current parsed text came from one specific partition. Match by
    // `dev_size` (sectors of the underlying device) — that uniquely picks
    // the PV record corresponding to *this* partition. Fall back to the
    // first PV when sizes don't match (LVM rounds dev_size; allow a small
    // tolerance).
    let candidates = named_blocks(pvs_block);
    let mut best: Option<(u64, String, String)> = None;
    let target_sectors = partition_bytes / 512;
    for (name, block) in candidates {
        let pv_uuid = parse_assignment_string(&block, "id").unwrap_or_default();
        let dev_size = parse_assignment_u64(&block, "dev_size").unwrap_or(0);
        let diff = dev_size.abs_diff(target_sectors);
        match best {
            Some((existing_diff, _, _)) if existing_diff <= diff => {}
            _ => best = Some((diff, name, pv_uuid)),
        }
    }
    best.map(|(_, name, uuid)| (name, uuid))
}

fn parse_logical_volumes(block: &str) -> Vec<LvmLogicalVolumeMeta> {
    let mut out = Vec::new();
    for (lv_name, lv_block) in named_blocks(block) {
        let segments = parse_segments(&lv_block);
        if segments.is_empty() {
            continue;
        }
        out.push(LvmLogicalVolumeMeta {
            name: lv_name,
            segments,
        });
    }
    out
}

fn parse_segments(lv_block: &str) -> Vec<LvmSegment> {
    let mut indexed: BTreeMap<u64, LvmSegment> = BTreeMap::new();
    for (seg_name, seg_block) in named_blocks(lv_block) {
        if !seg_name.starts_with("segment") {
            continue;
        }
        let order = seg_name
            .trim_start_matches("segment")
            .parse::<u64>()
            .unwrap_or(u64::MAX);
        let stripe_count = parse_assignment_u64(&seg_block, "stripe_count").unwrap_or(1);
        let extent_count = match parse_assignment_u64(&seg_block, "extent_count") {
            Some(value) => value,
            None => continue,
        };
        let Some((pv_local_id, pv_extent_offset)) = parse_first_stripe(&seg_block) else {
            continue;
        };
        indexed.insert(
            order,
            LvmSegment {
                pv_local_id,
                pv_extent_offset,
                extent_count,
                stripe_count,
            },
        );
    }
    indexed.into_values().collect()
}

fn parse_first_stripe(segment: &str) -> Option<(String, u64)> {
    let stripes_pos = segment.find("stripes")?;
    let after = &segment[stripes_pos..];
    let open = after.find('[')?;
    let close = after[open..].find(']')? + open;
    let inside = &after[open + 1..close];
    // Format: "pv0", 0, "pv1", 0
    let mut parts = inside.split(',').map(str::trim);
    let pv = parts.next()?.trim_matches('"').to_string();
    let extent = parts.next()?.parse::<u64>().ok()?;
    Some((pv, extent))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
contents = "Text Format Volume Group"
volum {
id = "zgsS1z-wV2Z-nGYB-9HUJ-VnYh-Psyi-Q3Rfq3"
seqno = 5
extent_size = 8192
physical_volumes {
pv0 {
id = "HwIOOP-kPve-aJ3C-3TFJ-Kcno-CaQU-cBg7oz"
dev_size = 58593280
pe_start = 2048
}
pv1 {
id = "uBbKbG-MxC5-I7pL-cG9j-6Y0S-QTRJ-HeyQdt"
dev_size = 58591232
pe_start = 2048
}
}
logical_volumes {
root {
id = "abc"
segment_count = 2
segment1 {
start_extent = 0
extent_count = 7152
type = "striped"
stripe_count = 1
stripes = [ "pv0", 0 ]
}
segment2 {
start_extent = 7152
extent_count = 7152
type = "striped"
stripe_count = 1
stripes = [ "pv1", 0 ]
}
}
}
}
"#;

    #[test]
    fn parses_two_segment_lv_in_order() {
        let block = top_level_vg(SAMPLE).expect("vg block");
        let lvs_block = extract_block(&block.1, "logical_volumes").expect("lv block");
        let lvs = parse_logical_volumes(&lvs_block);
        assert_eq!(lvs.len(), 1);
        assert_eq!(lvs[0].name, "root");
        assert_eq!(lvs[0].segments.len(), 2);
        assert_eq!(lvs[0].segments[0].pv_local_id, "pv0");
        assert_eq!(lvs[0].segments[0].extent_count, 7152);
        assert_eq!(lvs[0].segments[1].pv_local_id, "pv1");
    }

    #[test]
    fn picks_pv_record_by_dev_size() {
        let vg_block = top_level_vg(SAMPLE).expect("vg").1;
        let pvs_block = extract_block(&vg_block, "physical_volumes").expect("pvs");
        // 58593280 sectors * 512 = 30,000,800,000-ish bytes — pv0
        let pv0 = pick_pv_for_partition(&pvs_block, 58593280 * 512).expect("pv0");
        assert_eq!(pv0.0, "pv0");
        let pv1 = pick_pv_for_partition(&pvs_block, 58591232 * 512).expect("pv1");
        assert_eq!(pv1.0, "pv1");
    }

    #[test]
    fn parses_extent_and_pe_start() {
        let block = top_level_vg(SAMPLE).expect("vg").1;
        assert_eq!(parse_assignment_u64(&block, "extent_size"), Some(8192));
        let pvs_block = extract_block(&block, "physical_volumes").expect("pvs");
        let pv0 = named_blocks(&pvs_block)
            .into_iter()
            .find(|(name, _)| name == "pv0")
            .expect("pv0 block");
        assert_eq!(parse_assignment_u64(&pv0.1, "pe_start"), Some(2048));
    }

    /// CentOS 7 default install: `vgcreate` writes seqno=2 with only
    /// `swap`, then each subsequent `lvcreate` appends a new snapshot.
    /// Reading the first match in byte order picks the seqno=2 snapshot
    /// and loses every LV created later. The active VG is the
    /// highest-seqno block.
    #[test]
    fn top_level_vg_picks_highest_seqno_snapshot() {
        const MULTI_SNAPSHOT: &str = r#"
centos {
id = "WPXnWM-k23X-u5FH-cdcm-aEC6-7pyr-4GkNk2"
seqno = 2
extent_size = 8192
physical_volumes {
pv0 {
id = "RF7KpE-KD9I-QdI9-eeTE-09ZB-OEKT-WrkOyM"
dev_size = 144701440
pe_start = 2048
}
}
logical_volumes {
swap {
segment_count = 1
segment1 {
start_extent = 0
extent_count = 512
type = "striped"
stripe_count = 1
stripes = [ "pv0", 0 ]
}
}
}
}
centos {
id = "WPXnWM-k23X-u5FH-cdcm-aEC6-7pyr-4GkNk2"
seqno = 4
extent_size = 8192
physical_volumes {
pv0 {
id = "RF7KpE-KD9I-QdI9-eeTE-09ZB-OEKT-WrkOyM"
dev_size = 144701440
pe_start = 2048
}
}
logical_volumes {
swap {
segment_count = 1
segment1 {
start_extent = 0
extent_count = 512
type = "striped"
stripe_count = 1
stripes = [ "pv0", 0 ]
}
}
home {
segment_count = 1
segment1 {
start_extent = 512
extent_count = 4096
type = "striped"
stripe_count = 1
stripes = [ "pv0", 512 ]
}
}
root {
segment_count = 1
segment1 {
start_extent = 4608
extent_count = 12959
type = "striped"
stripe_count = 1
stripes = [ "pv0", 4608 ]
}
}
}
}
"#;
        let (name, block) = top_level_vg(MULTI_SNAPSHOT).expect("active vg");
        assert_eq!(name, "centos");
        assert_eq!(parse_assignment_u64(&block, "seqno"), Some(4));
        let lvs_block = extract_block(&block, "logical_volumes").expect("lvs");
        let mut lvs: Vec<String> = parse_logical_volumes(&lvs_block)
            .into_iter()
            .map(|lv| lv.name)
            .collect();
        lvs.sort();
        assert_eq!(lvs, vec!["home", "root", "swap"]);
    }
}
