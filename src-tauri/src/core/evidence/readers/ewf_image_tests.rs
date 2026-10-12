use super::*;
use md5::{Digest, Md5};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("vendor/ewf/tests/data")
        .join(name)
}

#[test]
fn ewf_real_ntfs_media_matches_acquisition_hash() {
    let reader = EwfHandle::open(&fixture("ntfs_sample.E01")).unwrap();
    let mut bytes = vec![0; reader.media_size() as usize];
    assert_eq!(reader.read_at(0, &mut bytes).unwrap(), bytes.len());
    assert_eq!(
        hex::encode(Md5::digest(&bytes)),
        "e4e9578a9c8bf6c6b375a80c630196a4"
    );
    assert_eq!(
        reader.acquiry_info().stored_md5.as_deref(),
        Some("e4e9578a9c8bf6c6b375a80c630196a4")
    );
    assert_eq!(
        (reader.bytes_per_sector(), &bytes[3..11]),
        (512, b"NTFS    ".as_slice())
    );
    let mut boundary = [0; 1024];
    reader.read_at(32760, &mut boundary).unwrap();
    assert_eq!(boundary.as_slice(), &bytes[32760..33784]);
    assert_eq!(
        reader.read_at(reader.media_size(), &mut boundary).unwrap(),
        0
    );
}

#[test]
fn ewf_multisegment_snapshots_and_hash_remain_complete() {
    let reader = EwfHandle::open(&fixture("multiseg_v1.E01")).unwrap();
    assert_eq!(reader.segment_file_snapshots().len(), 8);
    assert_eq!(reader.acquiry_info().segment_count, Some(8));
    assert_eq!(
        reader.segments_on_disk_bytes(),
        Some(
            reader
                .segment_file_snapshots()
                .iter()
                .map(|entry| entry.len)
                .sum()
        )
    );
    let mut digest = Md5::new();
    let mut offset = 0;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = reader.read_at(offset, &mut buffer).unwrap();
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        offset += count as u64;
    }
    assert_eq!(offset, reader.media_size());
    assert_eq!(
        Some(hex::encode(digest.finalize())),
        reader.acquiry_info().stored_md5.clone()
    );
}

#[test]
fn ewf_v2_compressed_and_raw_media_are_identical() {
    for name in ["zeros_128s.Ex01", "zeros_128s_compressed.Ex01"] {
        let reader = EwfHandle::open(&fixture(name)).unwrap();
        assert_eq!(
            (reader.media_size(), reader.bytes_per_sector()),
            (65536, 512)
        );
        let mut bytes = vec![1; 65536];
        assert_eq!(reader.read_at(0, &mut bytes).unwrap(), 65536);
        assert_eq!(bytes, vec![0; 65536]);
    }
}

#[test]
fn ewf_missing_segment_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("case.E01");
    std::fs::copy(fixture("multiseg_v1.E01"), &first).unwrap();
    assert!(EwfHandle::open(&first)
        .unwrap_err()
        .to_string()
        .contains("incomplete"));
    std::fs::copy(
        fixture("multiseg_v1.E03"),
        directory.path().join("case.E03"),
    )
    .unwrap();
    assert!(EwfHandle::open(&first).is_err());
}

#[test]
fn ewf_unicode_and_glob_characters_in_paths_are_literal() {
    let directory = tempfile::tempdir().unwrap();
    let nested = directory.path().join("案件[1]");
    std::fs::create_dir(&nested).unwrap();
    let path = nested.join("证据[01].EX01");
    std::fs::copy(fixture("zeros_128s_compressed.Ex01"), &path).unwrap();
    let reader = EwfHandle::open(&path).unwrap();
    assert_eq!(reader.media_size(), 65536);
    let renamed = nested.join("证据.bin");
    std::fs::rename(path, &renamed).unwrap();
    assert_eq!(EwfHandle::open(&renamed).unwrap().media_size(), 65536);
}

#[test]
fn ewf_corruption_reaches_existing_read_fault_reporting() {
    let original = fixture("ntfs_sample.E01");
    let probe = ewf::EwfReader::open(&original).unwrap();
    let (offset, _, compressed, _) = probe.debug_chunk(0).unwrap();
    assert!(compressed);
    let directory = tempfile::tempdir().unwrap();
    let damaged = directory.path().join("damaged.E01");
    let mut bytes = std::fs::read(original).unwrap();
    bytes[offset as usize] = 0; // Invalid zlib header, leaving all tables intact.
    std::fs::write(&damaged, bytes).unwrap();
    let reader = EwfHandle::open(&damaged).unwrap();
    assert!(reader.read_at(0, &mut [0; 512]).is_err());
    let image = crate::core::evidence::image::DiskImage::open(&damaged).unwrap();
    let (result, report) = image.collect_read_faults(|| image.read_range(0, 512));
    assert_eq!(result.unwrap(), vec![0; 512]);
    assert!(report.read_faults_detected);
    assert_eq!(report.synthetic_bytes, 512);
}
