use super::*;

#[test]
fn lvm_partition_is_auto_snapshot_supported() {
    assert!(snapshot_supported_plain_partition(PartitionKind::Lvm));
}

#[test]
fn non_browsable_partition_kinds_are_not_auto_snapshotted() {
    for kind in [
        PartitionKind::Apm,
        PartitionKind::BiosBoot,
        PartitionKind::Bsd,
        PartitionKind::Extended,
        PartitionKind::Luks,
        PartitionKind::Mdadm,
        PartitionKind::Swap,
        PartitionKind::Unknown,
    ] {
        assert!(!snapshot_supported_plain_partition(kind), "{kind:?}");
    }
}
