//! LVM member enumeration and RAID member sizing.
use super::*;

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

pub(super) fn group_pvs_into_vgs(pvs: &[LvmPvMetadata]) -> Vec<RaidVolumeGroup> {
    let mut grouped: BTreeMap<String, Vec<&LvmPvMetadata>> = BTreeMap::new();
    for pv in pvs {
        grouped.entry(pv.vg_uuid.clone()).or_default().push(pv);
    }
    grouped
        .into_iter()
        .map(|(vg_uuid, members)| {
            let head = members[0];
            let lvs = head
                .logical_volumes
                .iter()
                .map(|lv| RaidLogicalVolume {
                    name: lv.name.clone(),
                    size_bytes: lv
                        .segments
                        .iter()
                        .map(|seg| seg.extent_count * head.extent_size_sectors * 512)
                        .sum(),
                    segments: lv
                        .segments
                        .iter()
                        .filter_map(|seg| {
                            let pv = members.iter().find(|p| p.pv_local_id == seg.pv_local_id)?;
                            let byte_offset = pv.partition_byte_offset
                                + (pv.pe_start_sectors
                                    + seg.pv_extent_offset * pv.extent_size_sectors)
                                    * pv.sector_size as u64;
                            let byte_length =
                                seg.extent_count * pv.extent_size_sectors * pv.sector_size as u64;
                            Some(RaidLvSegment {
                                pv_local_id: seg.pv_local_id.clone(),
                                image_id: pv.image_id.clone(),
                                partition_id: pv.partition_id,
                                byte_offset,
                                byte_length,
                            })
                        })
                        .collect(),
                })
                .collect();
            RaidVolumeGroup {
                vg_uuid,
                vg_name: head.vg_name.clone(),
                extent_size_sectors: head.extent_size_sectors,
                members: members
                    .iter()
                    .map(|pv| RaidVgMember {
                        image_id: pv.image_id.clone(),
                        partition_id: pv.partition_id,
                        pv_local_id: pv.pv_local_id.clone(),
                        pv_uuid: pv.pv_uuid.clone(),
                        pe_start_sectors: pv.pe_start_sectors,
                        partition_byte_offset: pv.partition_byte_offset,
                        partition_byte_length: pv.partition_byte_length,
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
                total += seg.byte_length;
                segments.push(LvByteSegment {
                    image: image.clone(),
                    byte_offset: seg.byte_offset,
                    byte_length: seg.byte_length,
                });
            }
            if segments.is_empty() {
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
