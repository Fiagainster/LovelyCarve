//! LVM member enumeration and RAID member sizing.
use super::*;
#[cfg(test)]
#[path = "lvm_tests.rs"]
mod lvm_tests;

pub(super) fn assembled_bytes_for(level: i32, raid_disks: u32, per_member_bytes: u64) -> u64 {
    match level {
        // RAID0: capacity is the sum of all members. We store the head
        // member's `data_size_sectors` here as an approximation; mdadm
        // requires equal-sized members, so this is exact in practice.
        0 => per_member_bytes.saturating_mul(raid_disks as u64),
        // RAID5: one disk's worth of capacity goes to parity.
        5 => per_member_bytes.saturating_mul(raid_disks.saturating_sub(1) as u64),
        // RAID1: capacity equals one mirror member.
        1 => per_member_bytes,
        // Anything else — RAID6, RAID10, linear — gets the conservative
        // single-disk size so the report still has a non-zero number.
        _ => per_member_bytes,
    }
}

pub(super) fn collect_striped_members(
    members: &[AssembledMember],
    raid_disks: u32,
) -> anyhow::Result<Vec<StripedMember>> {
    // Build a slot → member map then materialise in slot order so the
    // striping math indexes members by slot, not by `dev_number`.
    let mut by_slot: Vec<Option<&AssembledMember>> = vec![None; raid_disks as usize];
    for member in members {
        let Some(slot) = member.sb.slot_index() else {
            continue;
        };
        let idx = slot as usize;
        if idx >= by_slot.len() {
            anyhow::bail!("member slot {slot} exceeds raid_disks={raid_disks}");
        }
        if by_slot[idx].is_some() {
            anyhow::bail!("two members claim slot {slot}");
        }
        by_slot[idx] = Some(member);
    }
    let mut out = Vec::with_capacity(raid_disks as usize);
    for (slot, slot_member) in by_slot.into_iter().enumerate() {
        let Some(member) = slot_member else {
            anyhow::bail!("slot {slot} has no member");
        };
        out.push(StripedMember {
            reader: member.reader.clone(),
            data_offset_bytes: member.sb.data_offset_sectors.saturating_mul(512),
            data_size_bytes: member.sb.data_size_sectors.saturating_mul(512),
        });
    }
    Ok(out)
}

pub(super) fn group_pvs_into_vgs(
    pvs: &[LvmPvMetadata],
    diagnostics: &mut Vec<String>,
) -> Vec<RaidVolumeGroup> {
    let mut grouped: BTreeMap<String, Vec<&LvmPvMetadata>> = BTreeMap::new();
    for pv in pvs {
        grouped.entry(pv.vg_uuid.clone()).or_default().push(pv);
    }
    grouped
        .into_iter()
        .map(|(vg_uuid, members)| {
            let head = members
                .iter()
                .max_by_key(|pv| pv.volume_group.sequence_number)
                .copied()
                .unwrap_or(members[0]);
            let group = &head.volume_group;
            let conflicting = members.iter().any(|pv| {
                pv.volume_group.sequence_number == group.sequence_number
                    && pv.volume_group != *group
            });
            if conflicting {
                diagnostics.push(format!(
                    "VG {} has conflicting copies at seqno {}",
                    group.name, group.sequence_number
                ));
            }
            let lvs = group
                .logical_volumes
                .iter()
                .filter_map(|lv| {
                    let size_bytes = lv.segments.iter().try_fold(0u64, |total, segment| {
                        segment
                            .extent_count
                            .checked_mul(group.extent_size_sectors)?
                            .checked_mul(512)?
                            .checked_add(total)
                    });
                    let Some(size_bytes) = size_bytes else {
                        diagnostics.push(format!("LVM {} size overflow", lv.name));
                        return None;
                    };
                    let mut segments = Vec::new();
                    for segment in &lv.segments {
                        if conflicting {
                            break;
                        }
                        if segment.kind != "striped"
                            || segment.stripe_count != 1
                            || segment.stripes.len() != 1
                        {
                            diagnostics.push(format!(
                                "LVM {}: unsupported segment {} with {} stripes",
                                lv.name, segment.kind, segment.stripe_count
                            ));
                            segments.clear();
                            break;
                        }
                        let stripe = &segment.stripes[0];
                        let Some(pv_definition) = group
                            .physical_volumes
                            .iter()
                            .find(|pv| pv.name == stripe.physical_volume)
                        else {
                            segments.clear();
                            break;
                        };
                        let candidates: Vec<_> = members
                            .iter()
                            .filter(|pv| pv.pv_uuid == pv_definition.identifier)
                            .collect();
                        if candidates.len() != 1 {
                            diagnostics.push(format!(
                                "LVM {}: missing or ambiguous PV {}",
                                lv.name, pv_definition.identifier
                            ));
                            segments.clear();
                            break;
                        }
                        let pv = candidates[0];
                        if pv.pe_start_sectors != pv_definition.pe_start_sectors {
                            diagnostics.push(format!(
                                "LVM {}: PV {} PE start conflicts with its label",
                                lv.name, pv.pv_uuid
                            ));
                            segments.clear();
                            break;
                        }
                        let mut single = segment.clone();
                        single.start_extent = 0;
                        let volume = crate::core::evidence::partitions::lvm2::text::LogicalVolume {
                            name: lv.name.clone(),
                            identifier: lv.identifier.clone(),
                            segments: vec![single],
                        };
                        let layout = crate::core::evidence::partitions::lvm2::layout::linear_layout(
                            group,
                            &volume,
                            pv_definition,
                            pv.partition_byte_offset,
                            pv.partition_byte_length.min(pv.pv_size_bytes),
                        );
                        let layout = match layout {
                            Ok(layout) => layout,
                            Err(error) => {
                                diagnostics.push(format!("LVM {}: {error:#}", lv.name));
                                segments.clear();
                                break;
                            }
                        };
                        let Some(range) = layout.segments.first() else {
                            segments.clear();
                            break;
                        };
                        if segments.iter().any(|previous: &RaidLvSegment| {
                            previous.image_id == pv.image_id
                                && previous.partition_id == pv.partition_id
                                && previous.byte_offset < range.physical_offset + range.size
                                && range.physical_offset
                                    < previous.byte_offset + previous.byte_length
                        }) {
                            diagnostics
                                .push(format!("LVM {}: overlapping physical extents", lv.name));
                            segments.clear();
                            break;
                        }
                        segments.push(RaidLvSegment {
                            pv_local_id: pv_definition.name.clone(),
                            image_id: pv.image_id.clone(),
                            partition_id: pv.partition_id,
                            byte_offset: range.physical_offset,
                            byte_length: range.size,
                        });
                    }
                    Some(RaidLogicalVolume {
                        name: lv.name.clone(),
                        size_bytes,
                        segments,
                    })
                })
                .collect();
            RaidVolumeGroup {
                vg_uuid,
                vg_name: head.vg_name.clone(),
                extent_size_sectors: head.extent_size_sectors,
                members: members
                    .iter()
                    .filter_map(|pv| {
                        let definition = group
                            .physical_volumes
                            .iter()
                            .find(|definition| definition.identifier == pv.pv_uuid)?;
                        Some(RaidVgMember {
                            image_id: pv.image_id.clone(),
                            partition_id: pv.partition_id,
                            pv_local_id: definition.name.clone(),
                            pv_uuid: pv.pv_uuid.clone(),
                            pe_start_sectors: definition.pe_start_sectors,
                            partition_byte_offset: pv.partition_byte_offset,
                            partition_byte_length: pv.partition_byte_length,
                        })
                    })
                    .collect(),
                logical_volumes: lvs,
            }
        })
        .collect()
}

pub(super) fn materialise_logical_volumes(
    images: &[Arc<DiskImage>],
    vgs: &[RaidVolumeGroup],
) -> Vec<LinearLvSource> {
    let mut out = Vec::new();
    for vg in vgs {
        for lv in &vg.logical_volumes {
            let mut segments = Vec::with_capacity(lv.segments.len());
            let mut total: u64 = 0;
            for seg in &lv.segments {
                let Some(image) = images.iter().find(|img| img.id == seg.image_id) else {
                    continue;
                };
                let Some(next_total) = total.checked_add(seg.byte_length) else {
                    segments.clear();
                    break;
                };
                if seg
                    .byte_offset
                    .checked_add(seg.byte_length)
                    .is_none_or(|end| end > image.len())
                {
                    segments.clear();
                    break;
                }
                total = next_total;
                segments.push(LvByteSegment {
                    image: image.clone(),
                    byte_offset: seg.byte_offset,
                    byte_length: seg.byte_length,
                });
            }
            if segments.is_empty() || total != lv.size_bytes {
                continue;
            }
            out.push(LinearLvSource {
                vg_uuid: vg.vg_uuid.clone(),
                vg_name: vg.vg_name.clone(),
                lv_name: lv.name.clone(),
                size_bytes: total,
                segments,
            });
        }
    }
    out
}

pub(super) fn enumerate_candidates(
    images: &[Arc<DiskImage>],
    lv_devices: &[LinearLvSource],
) -> Vec<CandidateMember> {
    let mut out = Vec::with_capacity(lv_devices.len());
    for lv in lv_devices {
        let reader: MemberReader = Arc::new(lv.clone());
        out.push(CandidateMember {
            reader,
            origin: RaidMemberOrigin::Lv {
                vg_uuid: lv.vg_uuid.clone(),
                vg_name: lv.vg_name.clone(),
                lv_name: lv.lv_name.clone(),
                size_bytes: lv.size_bytes,
            },
        });
    }
    for image in images {
        let sector_size = image.sector_size as u64;
        for partition in &image.partitions {
            if partition.kind != PartitionKind::Mdadm {
                continue;
            }
            let byte_offset = partition.start_lba.saturating_mul(sector_size);
            let byte_length = partition.length_lba.saturating_mul(sector_size);
            let member = WholeDiskMember {
                image: image.clone(),
                byte_offset,
                byte_length,
            };
            out.push(CandidateMember {
                reader: Arc::new(member),
                origin: RaidMemberOrigin::Whole {
                    image_id: image.id.clone(),
                    partition_id: partition.id,
                    byte_offset,
                    byte_length,
                },
            });
        }
    }
    out
}

pub(super) fn describe_origin(origin: &RaidMemberOrigin) -> String {
    match origin {
        RaidMemberOrigin::Lv {
            vg_name, lv_name, ..
        } => format!("LV {vg_name}/{lv_name}"),
        RaidMemberOrigin::Whole {
            image_id,
            partition_id,
            ..
        } => format!("image {image_id} partition {partition_id}"),
    }
}

#[derive(Clone)]
pub(super) struct AssembledMember {
    pub(super) sb: MdadmSuperblock,
    pub(super) origin: RaidMemberOrigin,
    pub(super) reader: MemberReader,
}

/// One enumerated RAID-member candidate. Both LV-on-LVM and whole-disk
/// shapes converge on this struct so the probe loop in
/// `detect_with_sources` runs once over a uniform input list.
pub(super) struct CandidateMember {
    pub(super) reader: MemberReader,
    pub(super) origin: RaidMemberOrigin,
}
