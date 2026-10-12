//! RAID assembly from explicit geometry and ordered members.
use super::*;

/// Assemble a RAID device from an explicit geometry and member order, detecting
/// the partition table / filesystem on the result. Used by the manual builder:
/// the parameters come from the examiner, so nothing is searched — the device
/// is materialised exactly as specified and probed so the UI can preview what
/// the geometry yields (and the caller can register it as a synthetic image).
pub fn manual_assemble(
    images: &[Arc<DiskImage>],
    params: &ManualRaidParams,
) -> anyhow::Result<AssembledArray> {
    if params.member_image_ids.is_empty() {
        anyhow::bail!("no members selected");
    }
    let mut members = Vec::with_capacity(params.member_image_ids.len());
    let mut min_size = u64::MAX;
    for id in &params.member_image_ids {
        let image = images
            .iter()
            .find(|im| &im.id == id)
            .with_context(|| format!("member image {id} is not loaded"))?;
        min_size = min_size.min(image.len());
        members.push(image.clone());
    }
    let chunk_bytes = params.chunk_sectors as u64 * 512;
    if chunk_bytes == 0 {
        anyhow::bail!("chunk size must be non-zero");
    }
    let data_offset = params.data_offset_sectors.saturating_mul(512);
    if data_offset >= min_size {
        anyhow::bail!("data offset {data_offset} exceeds member size {min_size}");
    }
    let level = RaidLevel::from_raw(params.level, params.layout).with_context(|| {
        format!(
            "unsupported RAID level/layout {}/{}",
            params.level, params.layout
        )
    })?;
    let per_member = min_size - data_offset;
    let n = members.len() as u64;
    let capacity = match params.level {
        0 => per_member.saturating_mul(n),
        5 => per_member.saturating_mul(n.saturating_sub(1)),
        _ => per_member,
    };
    let striped: Vec<StripedMember> = members
        .iter()
        .map(|im| StripedMember {
            reader: Arc::new(WholeDiskMember {
                image: im.clone(),
                byte_offset: 0,
                byte_length: im.len(),
            }),
            data_offset_bytes: data_offset,
            data_size_bytes: per_member,
        })
        .collect();
    let missing_slot = params.missing_slot.filter(|&s| s < striped.len());
    let source: Arc<dyn SectorSource + Send + Sync> = Arc::new(RaidVolume::set_degraded(
        level,
        512,
        chunk_bytes,
        Some(capacity),
        striped,
        missing_slot,
    ));

    let detected = detect_partitions_with_meta(source.as_ref(), 512)?;
    let mut partitions = detected.partitions;
    // Best effort: an unreadable boot region keeps the table-supplied kinds.
    if let Err(err) = refine_partition_kinds(source.as_ref(), &mut partitions, 512) {
        tracing::debug!("assembled RAID partition kind refinement failed: {err:#}");
    }
    let fs_kind = partitions
        .iter()
        .filter(|p| is_browsable_fs(p.kind))
        .max_by_key(|p| p.length_lba)
        .map(|p| p.kind)
        .unwrap_or(PartitionKind::Unknown);

    Ok(AssembledArray {
        set_uuid: manual_set_uuid(params),
        set_name: format!("manual-raid{}", params.level),
        source,
        size_bytes: capacity,
        fs_kind,
        partitions: if partitions.is_empty() {
            None
        } else {
            Some(partitions)
        },
    })
}

/// Whether a kind names a browsable filesystem (used to pick the assembled
/// device's primary `fs_kind` for the synthetic image label).
pub(super) fn is_browsable_fs(kind: PartitionKind) -> bool {
    matches!(
        kind,
        PartitionKind::Ext4
            | PartitionKind::Btrfs
            | PartitionKind::Xfs
            | PartitionKind::Ntfs
            | PartitionKind::Refs
            | PartitionKind::Fat12
            | PartitionKind::Fat16
            | PartitionKind::Fat32
            | PartitionKind::ExFat
    )
}

/// Deterministic `set_uuid` for a manual reconstruction so re-running the same
/// geometry reuses the same synthetic image instead of stacking duplicates.
pub(super) fn manual_set_uuid(params: &ManualRaidParams) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for id in &params.member_image_ids {
        id.hash(&mut hasher);
    }
    params.level.hash(&mut hasher);
    params.layout.hash(&mut hasher);
    params.chunk_sectors.hash(&mut hasher);
    params.data_offset_sectors.hash(&mut hasher);
    params.missing_slot.hash(&mut hasher);
    let h = hasher.finish();
    format!("manual{h:016x}{h:016x}")
}
