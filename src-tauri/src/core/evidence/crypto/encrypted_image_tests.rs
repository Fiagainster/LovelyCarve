use std::{path::PathBuf, sync::Arc};

use tokio_util::sync::CancellationToken;

use crate::core::evidence::crypto::encrypted_image::{unlock, EncryptionScheme, UnlockOptions};
use crate::core::evidence::image::{DiskImage, ImageFormat};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/encrypted")
        .join(name)
}

fn options(scheme: EncryptionScheme) -> UnlockOptions {
    UnlockOptions {
        target: Default::default(),
        scheme,
        password: "lovelycarve-test".into(),
        partition_id: None,
        offset: None,
        length: None,
        hidden: false,
        pim: if scheme == EncryptionScheme::Veracrypt {
            1
        } else {
            0
        },
    }
}

#[test]
fn independent_encrypted_fixtures_match_plaintext_and_detect_fat() {
    let expected = std::fs::read(fixture("plain.img")).unwrap();
    for (name, scheme, hidden) in [
        ("luks1.img", EncryptionScheme::Luks, false),
        ("luks2.img", EncryptionScheme::Luks, false),
        ("veracrypt.hc", EncryptionScheme::Veracrypt, false),
        ("hidden.hc", EncryptionScheme::Veracrypt, true),
    ] {
        let source = Arc::new(DiskImage::open(&fixture(name)).unwrap());
        let mut options = options(scheme);
        options.hidden = hidden;
        let (image, result) = unlock(&source, options, CancellationToken::new()).unwrap();
        assert_eq!(
            image.read_range(0, expected.len()).unwrap(),
            expected,
            "{name}"
        );
        assert_eq!(image.read_range(507, 1031).unwrap(), expected[507..1538]);
        assert_eq!(result.image.source_format, ImageFormat::Decrypted);
        assert_eq!(image.partitions.len(), 1);
        assert_eq!(result.source_length, source.len());
        assert!(image.read_range(image.len(), 1).unwrap().is_empty());
        drop(source);
        assert!(
            image.read_range(0, 512).is_err(),
            "closed source must not yield fabricated plaintext"
        );
    }
}

#[test]
fn encrypted_region_is_bounded_and_can_start_inside_an_image() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("offset.img");
    let encrypted = std::fs::read(fixture("luks1.img")).unwrap();
    let mut bytes = vec![0; 4096];
    bytes.extend_from_slice(&encrypted);
    bytes.extend_from_slice(&[0x55; 512]);
    std::fs::write(&path, bytes).unwrap();
    let source = Arc::new(DiskImage::open(&path).unwrap());
    let mut bounded = options(EncryptionScheme::Luks);
    bounded.offset = Some(4096);
    bounded.length = Some(encrypted.len() as u64);
    let (image, _) = unlock(&source, bounded, CancellationToken::new()).unwrap();
    assert_eq!(
        image.read_range(0, image.len() as usize).unwrap(),
        std::fs::read(fixture("plain.img")).unwrap()
    );
    let mut outside = options(EncryptionScheme::Luks);
    outside.offset = Some(u64::MAX);
    assert!(unlock(&source, outside, CancellationToken::new()).is_err());
}

#[test]
fn wrong_password_cancellation_and_truncation_fail_closed() {
    let source = Arc::new(DiskImage::open(&fixture("luks1.img")).unwrap());
    let mut wrong = options(EncryptionScheme::Luks);
    wrong.password = "wrong".into();
    assert!(unlock(&source, wrong, CancellationToken::new()).is_err());
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(unlock(&source, options(EncryptionScheme::Luks), cancelled).is_err());
    let mut truncated = options(EncryptionScheme::Luks);
    truncated.length = Some(source.len() - 1);
    assert!(unlock(&source, truncated, CancellationToken::new()).is_err());
    let mut conflict = options(EncryptionScheme::Luks);
    conflict.partition_id = Some(0);
    conflict.offset = Some(0);
    assert!(unlock(&source, conflict, CancellationToken::new()).is_err());
}

#[test]
fn corrupt_and_unbounded_luks2_headers_are_rejected_before_kdf() {
    for oversized in [false, true] {
        let mut bytes = std::fs::read(fixture("luks2.img")).unwrap();
        if oversized {
            bytes[8..16].copy_from_slice(&u64::MAX.to_be_bytes());
        } else {
            bytes[4200] ^= 1;
        }
        let mut reader = std::io::Cursor::new(&bytes);
        assert!(crate::core::evidence::crypto::luks_metadata::validate(
            &mut reader,
            bytes.len() as u64
        )
        .is_err());
    }
}

#[test]
fn attached_decryption_preserves_source_bytes_ids_and_snapshot_identity() {
    use crate::core::catalog::snapshot::volume_snapshot;
    use crate::core::filesystem::evidence_session::EvidenceSession;
    use crate::core::filesystem::tree::fs_tree;

    for name in ["luks1.img", "luks2.img", "veracrypt.hc"] {
        let scheme = if name.ends_with(".hc") {
            EncryptionScheme::Veracrypt
        } else {
            EncryptionScheme::Luks
        };
        let mut disk = DiskImage::open(&fixture(name)).unwrap();
        // Retain a nonzero source id so accidental part:0 remapping is detectable.
        if let Some(partition) = disk.partitions.first_mut() {
            partition.id = 2;
        }
        let disk = Arc::new(disk);
        let raw = disk.read_range(0, 4096).unwrap();
        let source = Arc::new(EvidenceSession::from(Arc::clone(&disk)));
        let token = CancellationToken::new();
        let (plain, result) = unlock(&disk, options(scheme), token.clone()).unwrap();
        let plain_size = plain.len();
        source
            .attach_decrypted_partition(2, plain, result.read_cancellation)
            .unwrap();
        token.cancel(); // The completed view outlives its initiating task.
        assert_eq!(source.read_range(0, 4096).unwrap(), raw);
        assert_eq!(source.info().id, disk.id);
        assert_eq!(source.info().unlocked_partitions, vec![2]);
        let children = fs_tree::children_for_image(&source, "part:2").unwrap();
        let hello = children
            .iter()
            .find(|node| node.name == "HELLO.TXT")
            .unwrap();
        assert!(hello.id.starts_with("fat:part:2:"));
        let bytes = fs_tree::read_file(&source, &hello.id).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(
            fs_tree::read_file_range(&source, &hello.id, 1, 5).unwrap(),
            bytes[1..6]
        );
        let boot = fs_tree::read_file_range(&source, "sysfile:part:2:0x0:0x200", 0, 512).unwrap();
        assert_eq!(boot, std::fs::read(fixture("plain.img")).unwrap()[..512]);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unlocked.dirz");
        let built = volume_snapshot::build_volume_snapshot(
            &source,
            "part:2",
            &path,
            100,
            "test".into(),
            &CancellationToken::new(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(built.header.source_volume_size, plain_size);
        let snapshot = volume_snapshot::load_snapshot(&path).unwrap();
        assert_eq!(snapshot.header.image_id, disk.id);
        assert!(snapshot
            .entries
            .iter()
            .any(|entry| entry.node.id == hello.id));
        source.revoke_decrypted_partitions();
        assert!(fs_tree::read_file(&source, &hello.id).is_err());
        assert_eq!(source.read_range(0, 4096).unwrap(), raw);
    }
}
