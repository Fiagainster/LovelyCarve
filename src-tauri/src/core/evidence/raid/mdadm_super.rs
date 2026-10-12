//! mdadm v1.x on-disk superblock decoder + probe registry.
//!
//! Companion to `partitions/mdadm.rs`: that module answers "is this byte
//! range an mdadm member?" for the partition-table dispatcher. RAID
//! assembly additionally needs the full superblock — `set_uuid`,
//! `level`, `chunksize`, `raid_disks`, `dev_number`, `data_offset`,
//! `dev_roles[]` — to align members and synthesise a virtual block
//! device.
//!
//! The file separates byte decoding from the parser registry used by `partitions.rs`:
//!
//! - Pure byte decoders (`parse_v1_2`) take a SB-candidate buffer and
//!   decode its fields, layout-matching the canonical Linux
//!   `struct mdp_superblock_1` (md_p.h).
//! - `MdadmSuperblockProbe` trait + static `PARSERS` table. Each impl
//!   knows where on a member device its variant's superblock lives,
//!   reads it through the unified `SectorSource` abstraction, and
//!   delegates byte decoding to the matching `parse_*` function. The
//!   dispatcher (`probe`) tries each entry in priority order and
//!   returns the first match.
//!
//! Currently registered: `V12Probe` (sector 8 / byte `0x1000`, the
//! modern default that real-world Linux RAID writes) and `V11Probe`
//! (sector 0 / byte `0`, the pre-1.2 layout). The v1.0 / v0.90
//! end-of-device trailers would slot in as additional impls without
//! changing any caller.

use std::io::{Cursor, SeekFrom};

use binrw::BinRead;

use crate::core::evidence::partitions::SectorSource;

const MDADM_MAGIC_LE: u32 = 0xA92B4EFC;

/// Decoded subset of the v1.x superblock. `dev_number` and `device_uuid`
/// distinguish members within an array; `set_uuid` + `set_name` group
/// members back into an array.
#[derive(Debug, Clone)]
pub struct MdadmSuperblock {
    pub major_version: u32,
    pub feature_map: u32,
    pub set_uuid: [u8; 16],
    pub set_name: String,
    /// `level` field. RAID0 = 0, RAID1 = 1, RAID5 = 5, RAID6 = 6,
    /// linear = -1 (`0xFFFFFFFF`).
    pub level: i32,
    pub layout: u32,
    /// Total array data size in 512-byte sectors.
    pub size_sectors: u64,
    /// Stripe size in 512-byte sectors. For RAID0 this is the chunk size.
    pub chunksize_sectors: u32,
    pub raid_disks: u32,
    /// Offset (in sectors from member-device start) where the actual data
    /// area begins. mdadm prepends a header region of this many sectors
    /// before stripe data.
    pub data_offset_sectors: u64,
    pub data_size_sectors: u64,
    pub device_uuid: [u8; 16],
    /// 0-based member index within the array.
    pub dev_number: u32,
    /// `dev_roles[]` array, indexed by `dev_number`. Each entry is one of
    /// `0..0xFFFD` (the slot-in-array index this member occupies),
    /// `0xFFFE` (faulty), or `0xFFFF` (spare). Needed because mdadm reuses
    /// dev_number across array events: when a spare replaces a failed
    /// member, the new member keeps its higher dev_number but takes over
    /// the failed member's slot. Without consulting this table, slot
    /// assignment is ambiguous for arrays that have ever seen a failure.
    pub dev_roles: Vec<u16>,
}

#[derive(Debug, BinRead)]
#[br(little)]
struct RawMdadmSuperblockV1 {
    magic: u32,
    major_version: u32,
    feature_map: u32,
    #[br(seek_before = SeekFrom::Start(16))]
    set_uuid: [u8; 16],
    set_name_raw: [u8; 32],
    #[br(seek_before = SeekFrom::Start(72))]
    level: i32,
    layout: u32,
    size_sectors: u64,
    chunksize_sectors: u32,
    raid_disks: u32,
    #[br(seek_before = SeekFrom::Start(128))]
    data_offset_sectors: u64,
    data_size_sectors: u64,
    #[br(seek_before = SeekFrom::Start(160))]
    dev_number: u32,
    #[br(seek_before = SeekFrom::Start(168))]
    device_uuid: [u8; 16],
    #[br(seek_before = SeekFrom::Start(220))]
    max_dev: u32,
}

impl MdadmSuperblock {
    /// Returns the array slot this member currently occupies, derived from
    /// `dev_roles[dev_number]`. `Some(slot)` for an active data/parity
    /// member; `None` for a faulty (`0xFFFE`) or spare (`0xFFFF`) entry,
    /// for an out-of-range `dev_number`, or when the slot is `>= raid_disks`
    /// (treated as malformed). Slot indices are 0-based and fit into
    /// `raid_disks`, matching the indexing used by RAID layout math.
    pub fn slot_index(&self) -> Option<u32> {
        let role = *self.dev_roles.get(self.dev_number as usize)?;
        match role {
            0xFFFE | 0xFFFF => None,
            slot => {
                let slot_u32 = slot as u32;
                if slot_u32 < self.raid_disks {
                    Some(slot_u32)
                } else {
                    None
                }
            }
        }
    }
}

pub fn parse_v1_2(buf: &[u8]) -> Option<MdadmSuperblock> {
    if buf.len() < 0x108 {
        return None;
    }
    let raw = RawMdadmSuperblockV1::read(&mut Cursor::new(buf)).ok()?;
    if raw.magic != MDADM_MAGIC_LE {
        return None;
    }
    if raw.major_version != 1 {
        return None;
    }
    let set_name = decode_cstring(&raw.set_name_raw);
    // Array-state + role section (192..256 + dev_roles[]):
    //   utime 192..200, events 200..208, resync_offset 208..216,
    //   sb_csum 216..220, max_dev 220..224, pad3 224..256,
    //   dev_roles 256.. (max_dev × __le16, 2 bytes each).
    // Cap `max_dev` so a corrupted SB can't cause us to walk past the buffer.
    let role_table_bytes = (raw.max_dev as usize).saturating_mul(2);
    let role_table_end = 256usize.saturating_add(role_table_bytes);
    let dev_roles = if role_table_end <= buf.len() {
        buf[256..role_table_end]
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(u16::from_le_bytes)
            .collect()
    } else {
        // Truncated SB: read whatever full role entries are present so the
        // caller still has the role for low-numbered members (which is the
        // common case).
        buf[256..]
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(u16::from_le_bytes)
            .collect()
    };

    Some(MdadmSuperblock {
        major_version: raw.major_version,
        feature_map: raw.feature_map,
        set_uuid: raw.set_uuid,
        set_name,
        level: raw.level,
        layout: raw.layout,
        size_sectors: raw.size_sectors,
        chunksize_sectors: raw.chunksize_sectors,
        raid_disks: raw.raid_disks,
        data_offset_sectors: raw.data_offset_sectors,
        data_size_sectors: raw.data_size_sectors,
        device_uuid: raw.device_uuid,
        dev_number: raw.dev_number,
        dev_roles,
    })
}

fn decode_cstring(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub fn format_uuid(uuid: &[u8; 16]) -> String {
    uuid.iter().map(|b| format!("{b:02x}")).collect()
}

/// Probe-object pattern for mdadm superblocks. Mirrors `PartParser` in
/// `partitions.rs`: each impl claims responsibility for one variant
/// (v1.2 sector-8, v1.1 sector-0, …), reads its expected SB region from
/// a `SectorSource`, and decodes with a `parse_*` function. The
/// dispatcher (`probe`) walks `PARSERS` in priority order and returns
/// the first impl that finds a valid superblock.
pub trait MdadmSuperblockProbe: Sync {
    /// Variant label used in diagnostic strings (`"1.2"`, `"1.1"`, …).
    fn version(&self) -> &'static str;
    /// Read and decode the SB candidate. `Ok(None)` on "not this
    /// variant"; `Err` only on read failure (caller surfaces it as a
    /// per-source diagnostic so probing the rest of the candidate set
    /// is not aborted by one bad LV / partition).
    fn probe(&self, source: &dyn SectorSource) -> anyhow::Result<Option<MdadmSuperblock>>;
}

const V12_OFFSET: u64 = 0x1000;
const V11_OFFSET: u64 = 0x0;
const SB_BUF_LEN: usize = 0x1000;
/// Smallest read past which `parse_v1_2` can decode all referenced
/// fields (constant section ends at byte 0x100; per-device section ends
/// at byte 0x108). Used by every probe to cheap-reject too-small
/// sources before issuing a read.
const SB_MIN_BYTES: u64 = 0x110;

struct V12Probe;
struct V11Probe;

impl MdadmSuperblockProbe for V12Probe {
    fn version(&self) -> &'static str {
        "1.2"
    }
    fn probe(&self, source: &dyn SectorSource) -> anyhow::Result<Option<MdadmSuperblock>> {
        if source.size() < V12_OFFSET + SB_MIN_BYTES {
            return Ok(None);
        }
        let buf = source.read(V12_OFFSET, SB_BUF_LEN)?;
        Ok(parse_v1_2(&buf))
    }
}

impl MdadmSuperblockProbe for V11Probe {
    fn version(&self) -> &'static str {
        "1.1"
    }
    fn probe(&self, source: &dyn SectorSource) -> anyhow::Result<Option<MdadmSuperblock>> {
        if source.size() < V11_OFFSET + SB_MIN_BYTES {
            return Ok(None);
        }
        let buf = source.read(V11_OFFSET, SB_BUF_LEN)?;
        Ok(parse_v1_2(&buf))
    }
}

/// Priority-ordered probe table. v1.2 (sector 8) is the modern Linux
/// default and the variant the existing case material in `D:\Code\raid`
/// uses, so it sits first; v1.1 (sector 0) follows so any pre-1.2
/// arrays still get assembled. Both decode through `parse_v1_2` because
/// the v1.0 / v1.1 / v1.2 byte layouts are identical — only the SB's
/// position on the member device changes.
static PARSERS: &[&dyn MdadmSuperblockProbe] = &[&V12Probe, &V11Probe];

/// Walk `PARSERS` and return the first variant that decodes a valid
/// superblock from `source`. The `&'static str` is the matching impl's
/// `version()` label so the caller can include it in diagnostics.
pub fn probe(source: &dyn SectorSource) -> anyhow::Result<Option<(MdadmSuperblock, &'static str)>> {
    for parser in PARSERS {
        if let Some(sb) = parser.probe(source)? {
            return Ok(Some((sb, parser.version())));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::evidence::partitions::test_support::BufSource;

    fn synthetic_v1_2_sb() -> Vec<u8> {
        let mut buf = vec![0u8; 0x1000];
        buf[0..4].copy_from_slice(&MDADM_MAGIC_LE.to_le_bytes());
        buf[4..8].copy_from_slice(&1u32.to_le_bytes());
        buf[16..32].copy_from_slice(&[0xAAu8; 16]);
        buf[32..32 + 6].copy_from_slice(b"mac:0\0");
        buf[72..76].copy_from_slice(&0i32.to_le_bytes()); // RAID0
        buf[80..88].copy_from_slice(&123_456_u64.to_le_bytes());
        buf[88..92].copy_from_slice(&1024u32.to_le_bytes()); // 512 KiB chunk
        buf[92..96].copy_from_slice(&2u32.to_le_bytes());
        buf[128..136].copy_from_slice(&67_584_u64.to_le_bytes());
        buf[136..144].copy_from_slice(&126_187_520_u64.to_le_bytes());
        buf[160..164].copy_from_slice(&0u32.to_le_bytes());
        buf[168..184].copy_from_slice(&[0xBBu8; 16]);
        // max_dev = 4, dev_roles = [0, 1, 0xFFFF, 0xFFFE].
        buf[220..224].copy_from_slice(&4u32.to_le_bytes());
        buf[256..258].copy_from_slice(&0u16.to_le_bytes());
        buf[258..260].copy_from_slice(&1u16.to_le_bytes());
        buf[260..262].copy_from_slice(&0xFFFFu16.to_le_bytes());
        buf[262..264].copy_from_slice(&0xFFFEu16.to_le_bytes());
        buf
    }

    #[test]
    fn decodes_canonical_v1_2_fields() {
        let buf = synthetic_v1_2_sb();
        let sb = parse_v1_2(&buf).expect("decode");
        assert_eq!(sb.major_version, 1);
        assert_eq!(sb.set_uuid, [0xAA; 16]);
        assert_eq!(sb.set_name, "mac:0");
        assert_eq!(sb.level, 0);
        assert_eq!(sb.chunksize_sectors, 1024);
        assert_eq!(sb.raid_disks, 2);
        assert_eq!(sb.data_offset_sectors, 67_584);
        assert_eq!(sb.data_size_sectors, 126_187_520);
        assert_eq!(sb.dev_number, 0);
    }

    #[test]
    fn rejects_wrong_magic() {
        let mut buf = synthetic_v1_2_sb();
        buf[0..4].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse_v1_2(&buf).is_none());
    }

    #[test]
    fn formats_uuid_lowercase_hex() {
        let uuid = [
            0x88, 0x0f, 0x20, 0x57, 0x30, 0xe6, 0x4d, 0xa6, 0x5b, 0xfd, 0xb0, 0x6f, 0x70, 0x01,
            0x31, 0xbb,
        ];
        assert_eq!(format_uuid(&uuid), "880f205730e64da65bfdb06f700131bb");
    }

    #[test]
    fn slot_index_resolves_via_role_table() {
        let mut buf = synthetic_v1_2_sb();
        // raid_disks=3, dev_number=3, dev_roles=[0,1,0xFFFF,2] — i.e. dev3
        // took over the slot-2 role after the original dev2 was marked spare.
        buf[92..96].copy_from_slice(&3u32.to_le_bytes());
        buf[160..164].copy_from_slice(&3u32.to_le_bytes());
        buf[220..224].copy_from_slice(&4u32.to_le_bytes());
        buf[256..258].copy_from_slice(&0u16.to_le_bytes());
        buf[258..260].copy_from_slice(&1u16.to_le_bytes());
        buf[260..262].copy_from_slice(&0xFFFFu16.to_le_bytes());
        buf[262..264].copy_from_slice(&2u16.to_le_bytes());
        let sb = parse_v1_2(&buf).expect("decode");
        assert_eq!(sb.slot_index(), Some(2));
    }

    #[test]
    fn slot_index_treats_spare_and_faulty_as_none() {
        let mut buf = synthetic_v1_2_sb();
        buf[160..164].copy_from_slice(&2u32.to_le_bytes()); // dev_number=2 (spare)
        let sb = parse_v1_2(&buf).expect("decode");
        assert_eq!(sb.slot_index(), None);

        let mut buf = synthetic_v1_2_sb();
        buf[160..164].copy_from_slice(&3u32.to_le_bytes()); // dev_number=3 (faulty)
        let sb = parse_v1_2(&buf).expect("decode");
        assert_eq!(sb.slot_index(), None);
    }

    /// Probe registry walks `PARSERS` in declaration order. The v1.2
    /// entry sits first, so a buffer with the canonical SB at byte
    /// 0x1000 must come back tagged "1.2".
    #[test]
    fn probe_picks_v12_variant_at_sector_8() {
        let mut buf = vec![0u8; 0x4000];
        buf[V12_OFFSET as usize..V12_OFFSET as usize + 0x1000]
            .copy_from_slice(&synthetic_v1_2_sb()[..0x1000]);
        let src = BufSource(buf);
        let (sb, version) = probe(&src).expect("probe ok").expect("matched");
        assert_eq!(version, "1.2");
        assert_eq!(sb.set_uuid, [0xAA; 16]);
    }

    /// Pre-1.2 layout: SB at sector 0 / byte 0. The v1.1 entry sits
    /// after v1.2, so this exercises the fallthrough path.
    #[test]
    fn probe_falls_through_to_v11_variant_at_sector_0() {
        let mut buf = vec![0u8; 0x4000];
        buf[..0x1000].copy_from_slice(&synthetic_v1_2_sb()[..0x1000]);
        let src = BufSource(buf);
        let (sb, version) = probe(&src).expect("probe ok").expect("matched");
        assert_eq!(version, "1.1");
        assert_eq!(sb.set_uuid, [0xAA; 16]);
    }

    /// Empty buffer must short-circuit on every variant's size check
    /// without panicking on the read.
    #[test]
    fn probe_returns_none_for_undersized_source() {
        let src = BufSource(vec![0u8; 0x100]);
        assert!(probe(&src).expect("probe ok").is_none());
    }
}
