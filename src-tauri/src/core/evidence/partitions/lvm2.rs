//! LVM2 physical-volume label detection.
//!
//! Scans four candidate sectors in the first 2048 bytes (offsets 0,
//! 512, 1024, 1536). At each candidate sector start, requires both the
//! `LABELONE` marker at byte 0 and the `LVM2 001` type tag at byte 24 —
//! the strict pair-anchored check rules out random data that happens to
//! contain only one of the two strings.

pub mod metadata;

use std::io::{Cursor, SeekFrom};

use binrw::BinRead;

use crate::core::evidence::partitions::{
    DetectCtx, PartParser, Partition, PartitionKind, TableScheme,
};

const LVM2_SCAN_OFFSETS: [usize; 4] = [0, 512, 1024, 1536];

pub struct Lvm2Parser;

#[derive(Debug, BinRead)]
#[br(little, magic = b"LABELONE")]
struct Lvm2LabelProbe {
    #[br(seek_before = SeekFrom::Start(24))]
    type_tag: [u8; 8],
}

impl PartParser for Lvm2Parser {
    fn name(&self) -> &'static str {
        "lvm2"
    }

    fn scheme(&self) -> TableScheme {
        TableScheme::Lvm2
    }

    fn try_parse(&self, ctx: &DetectCtx<'_>) -> anyhow::Result<Option<Vec<Partition>>> {
        for &off in &LVM2_SCAN_OFFSETS {
            let Some(probe) = ctx
                .head
                .get(off..)
                .and_then(|bytes| Lvm2LabelProbe::read(&mut Cursor::new(bytes)).ok())
            else {
                continue;
            };
            if probe.type_tag != *b"LVM2 001" {
                continue;
            }
            return Ok(Some(vec![Partition {
                id: 0,
                kind: PartitionKind::Lvm,
                start_lba: 0,
                length_lba: ctx.size / ctx.sector_size as u64,
                label: Some("LVM2 PV".into()),
                unique_guid: None,
            }]));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::evidence::partitions::{detect_partitions, test_support::BufSource};

    fn put_label(buf: &mut [u8], off: usize) {
        buf[off..off + 8].copy_from_slice(b"LABELONE");
        buf[off + 24..off + 32].copy_from_slice(b"LVM2 001");
    }

    #[test]
    fn detects_lvm2_label_at_sector_0() {
        let mut buf = vec![0u8; 4096];
        put_label(&mut buf, 0);
        let src = BufSource(buf);
        let parts = detect_partitions(&src, 512).expect("detect");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, PartitionKind::Lvm);
        assert_eq!(parts[0].label.as_deref(), Some("LVM2 PV"));
    }

    #[test]
    fn detects_lvm2_label_at_sector_1() {
        let mut buf = vec![0u8; 4096];
        put_label(&mut buf, 512);
        let src = BufSource(buf);
        let parts = detect_partitions(&src, 512).expect("detect");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, PartitionKind::Lvm);
    }

    #[test]
    fn detects_lvm2_label_at_sector_3() {
        let mut buf = vec![0u8; 4096];
        put_label(&mut buf, 1536);
        let src = BufSource(buf);
        let parts = detect_partitions(&src, 512).expect("detect");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, PartitionKind::Lvm);
    }

    #[test]
    fn rejects_labelone_without_lvm2_type() {
        let mut buf = vec![0u8; 4096];
        buf[0..8].copy_from_slice(b"LABELONE");
        let src = BufSource(buf);
        let parts = detect_partitions(&src, 512).expect("detect");
        assert!(parts.is_empty());
    }
}
