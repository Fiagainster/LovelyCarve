use super::*;

/// Hand-stitches a minimal LVM2 PV with three text snapshots, where the
/// `mda_header.location_descriptors[0]` points at the THIRD snapshot.
/// The seqno-only fallback would prefer whichever has the highest
/// `seqno`; the binary path must follow the on-disk pointer regardless.
#[test]
fn locate_active_metadata_region_follows_mda_header() {
    const PROBE_LEN: usize = 64 * 1024;
    const MDA_OFFSET: usize = 4096;
    const SNAPSHOT_1_OFFSET: usize = 512; // relative to MDA start
    const SNAPSHOT_2_OFFSET: usize = 4096;
    const SNAPSHOT_3_OFFSET: usize = 8192;
    const METADATA_AREA_SIZE: usize = 16 * 1024;

    let mut probe = vec![0u8; PROBE_LEN];

    // PV label header at offset 512.
    let label = &mut probe[512..544];
    label[0..8].copy_from_slice(b"LABELONE");
    label[20..24].copy_from_slice(&32u32.to_le_bytes()); // data_offset
    label[24..32].copy_from_slice(b"LVM2 001");

    // PhysicalVolumeHeader at offset 544 = 512 + 32. pv_ident(32) +
    // pv_size(8) = 40 bytes, then descriptor arrays. data_descriptors
    // is empty (just the 16-byte zero terminator), then the first
    // metadata_descriptor points at MDA_OFFSET.
    let pv_header_start = 544;
    let descriptors_start = pv_header_start + 40;
    // data_descriptors: empty → 16 zeros (already zero).
    let metadata_descriptors_start = descriptors_start + 16;
    probe[metadata_descriptors_start..metadata_descriptors_start + 8]
        .copy_from_slice(&(MDA_OFFSET as u64).to_le_bytes());
    probe[metadata_descriptors_start + 8..metadata_descriptors_start + 16]
        .copy_from_slice(&(METADATA_AREA_SIZE as u64).to_le_bytes());
    // metadata_descriptors terminator (16 zeros, already zero).

    // MetadataAreaHeader at MDA_OFFSET: checksum(4) + magic(16) +
    // version(4) + ma_offset(8) + ma_size(8) = 40 bytes.
    let mda_start = MDA_OFFSET;
    probe[mda_start + 4..mda_start + 20].copy_from_slice(b" LVM2 x[5A%r0N*>");
    probe[mda_start + 32..mda_start + 40]
        .copy_from_slice(&(METADATA_AREA_SIZE as u64).to_le_bytes());

    // First location_descriptor at MDA_OFFSET + 40, pointing at
    // snapshot 3 (the active one).
    let snapshot_3_text = "centos { id=\"x\" seqno=4 physical_volumes { pv0 { } } logical_volumes { swap { } home { } root { } } }";
    let snapshot_3_size = snapshot_3_text.len();
    let locn_start = mda_start + 40;
    probe[locn_start..locn_start + 8].copy_from_slice(&(SNAPSHOT_3_OFFSET as u64).to_le_bytes());
    probe[locn_start + 8..locn_start + 16].copy_from_slice(&(snapshot_3_size as u64).to_le_bytes());

    // Place all three snapshots into the metadata buffer. Snapshots 1
    // and 2 are decoys — the binary header must still pick #3.
    let snapshot_1_text =
        "centos { id=\"x\" seqno=2 physical_volumes { pv0 { } } logical_volumes { swap { } } }";
    let snapshot_2_text = "centos { id=\"x\" seqno=3 physical_volumes { pv0 { } } logical_volumes { swap { } home { } } }";
    let s1_abs = mda_start + SNAPSHOT_1_OFFSET;
    probe[s1_abs..s1_abs + snapshot_1_text.len()].copy_from_slice(snapshot_1_text.as_bytes());
    let s2_abs = mda_start + SNAPSHOT_2_OFFSET;
    probe[s2_abs..s2_abs + snapshot_2_text.len()].copy_from_slice(snapshot_2_text.as_bytes());
    let s3_abs = mda_start + SNAPSHOT_3_OFFSET;
    probe[s3_abs..s3_abs + snapshot_3_text.len()].copy_from_slice(snapshot_3_text.as_bytes());

    let (start, end) = locate_active_metadata_region(&probe).expect("binary locator");
    assert_eq!(start, s3_abs);
    assert_eq!(end, s3_abs + snapshot_3_size);
    let active_text = std::str::from_utf8(&probe[start..end]).unwrap();
    assert!(active_text.contains("seqno=4"));
    assert!(active_text.contains("root"));
}

/// When the binary headers are absent (corrupt/non-LVM bytes), the
/// locator must return `None` so callers fall back to text scanning.
#[test]
fn locate_active_metadata_region_rejects_non_lvm_bytes() {
    let probe = vec![0u8; 4096];
    assert!(locate_active_metadata_region(&probe).is_none());
}

/// Wrap-around in the rotating buffer (active text crossing the
/// metadata-area-size boundary) means there's no single contiguous
/// range — the locator returns `None` rather than a torn slice.
#[test]
fn locate_active_metadata_region_returns_none_on_wrap() {
    const PROBE_LEN: usize = 64 * 1024;
    const MDA_OFFSET: usize = 4096;
    const METADATA_AREA_SIZE: usize = 4096;
    let mut probe = vec![0u8; PROBE_LEN];

    let label = &mut probe[512..544];
    label[0..8].copy_from_slice(b"LABELONE");
    label[20..24].copy_from_slice(&32u32.to_le_bytes());
    label[24..32].copy_from_slice(b"LVM2 001");

    let metadata_descriptors_start = 544 + 40 + 16;
    probe[metadata_descriptors_start..metadata_descriptors_start + 8]
        .copy_from_slice(&(MDA_OFFSET as u64).to_le_bytes());
    probe[metadata_descriptors_start + 8..metadata_descriptors_start + 16]
        .copy_from_slice(&(METADATA_AREA_SIZE as u64).to_le_bytes());

    probe[MDA_OFFSET + 4..MDA_OFFSET + 20].copy_from_slice(b" LVM2 x[5A%r0N*>");
    probe[MDA_OFFSET + 32..MDA_OFFSET + 40]
        .copy_from_slice(&(METADATA_AREA_SIZE as u64).to_le_bytes());

    // data_area_offset 3000 + size 2000 = 5000 > metadata_area_size 4096.
    let locn_start = MDA_OFFSET + 40;
    probe[locn_start..locn_start + 8].copy_from_slice(&3000u64.to_le_bytes());
    probe[locn_start + 8..locn_start + 16].copy_from_slice(&2000u64.to_le_bytes());

    assert!(locate_active_metadata_region(&probe).is_none());
}
