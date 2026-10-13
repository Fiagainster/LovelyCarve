//! Metadata-driven RAID detection and assembly.
use super::*;

/// Same as `detect`, but additionally returns the assembled sources so
/// the caller can register them as synthetic `DiskImage`s. Reported
/// arrays have `assembled_image_id == None` here; the caller sets that
/// field after registration so the frontend can deep-link the assembled
/// device.
pub fn detect_with_sources(images: &[Arc<DiskImage>]) -> (RaidStack, Vec<AssembledArray>) {
    let mut diagnostics = Vec::new();

    // Pass 1 — collect LVM PV metadata across every loaded image.
    let mut image_summaries = Vec::with_capacity(images.len());
    let mut pvs: Vec<LvmPvMetadata> = Vec::new();
    for image in images {
        let mut pv_count = 0usize;
        for partition in &image.partitions {
            match lvm_meta::read_pv_metadata(image, partition) {
                Ok(Some(meta)) => {
                    pv_count += 1;
                    pvs.push(meta);
                }
                Ok(None) => {}
                Err(err) => diagnostics.push(format!(
                    "image {} partition {} PV read failed: {err}",
                    image.id, partition.id
                )),
            }
        }
        image_summaries.push(RaidImage {
            image_id: image.id.clone(),
            path: image.path.display().to_string(),
            size_bytes: image.len(),
            partition_count: image.partitions.len(),
            lvm_pv_count: pv_count,
        });
    }

    let volume_groups = group_pvs_into_vgs(&pvs, &mut diagnostics);
    let lv_devices = materialise_logical_volumes(images, &volume_groups);

    // Pass 2 — collect mdadm members. Both LV-on-LVM and whole-disk
    // members converge into a single `CandidateMember` list so the
    // probe loop runs once instead of twice; the probe registry in
    // `mdadm_super::probe` walks every registered SB variant (v1.2,
    // v1.1, …) and reports which one matched.
    let mut members_by_set: BTreeMap<String, Vec<AssembledMember>> = BTreeMap::new();
    let candidates = enumerate_candidates(images, &lv_devices);
    for candidate in candidates {
        match mdadm_super::probe(candidate.reader.as_ref()) {
            Ok(Some((sb, _version))) => {
                let set_uuid = mdadm_super::format_uuid(&sb.set_uuid);
                members_by_set
                    .entry(set_uuid)
                    .or_default()
                    .push(AssembledMember {
                        sb,
                        origin: candidate.origin,
                        reader: candidate.reader,
                    });
            }
            Ok(None) => {}
            Err(err) => diagnostics.push(format!(
                "mdadm probe failed on {}: {err}",
                describe_origin(&candidate.origin)
            )),
        }
    }

    // Pass 3 — assemble each set, probe filesystems, materialise readers.
    let mut arrays = Vec::new();
    let mut filesystems = Vec::new();
    let mut assembled = Vec::new();

    for (set_uuid, mut members) in members_by_set {
        // Slot-aware sort: missing slot (spare/faulty) sinks to the back so
        // the head member used for array-wide constants is always a real
        // active member when at least one is present.
        members.sort_by_key(|m| m.sb.slot_index().unwrap_or(u32::MAX));
        let head = match members.first() {
            Some(member) => member,
            None => continue,
        };
        let chunksize_sectors = head.sb.chunksize_sectors;
        let level = head.sb.level;
        let layout = head.sb.layout;
        let raid_disks = head.sb.raid_disks;
        let data_offset_sectors = head.sb.data_offset_sectors;
        let set_name = head.sb.set_name.clone();
        let assembled_bytes = assembled_bytes_for(
            level,
            raid_disks,
            head.sb.data_size_sectors.saturating_mul(512),
        );
        let array_members: Vec<RaidArrayMember> = members
            .iter()
            .map(|m| RaidArrayMember {
                dev_number: m.sb.dev_number,
                slot: m.sb.slot_index(),
                device_uuid: mdadm_super::format_uuid(&m.sb.device_uuid),
                data_size_sectors: m.sb.data_size_sectors,
                reconstructed: false,
                origin: m.origin.clone(),
            })
            .collect();
        arrays.push(RaidArray {
            set_uuid: set_uuid.clone(),
            set_name: set_name.clone(),
            level,
            layout,
            chunksize_sectors,
            raid_disks,
            data_offset_sectors,
            assembled_bytes,
            members: array_members,
            assembled_image_id: None,
            heuristic: false,
        });

        // Only materialise when every active slot is filled. With fewer
        // members than `raid_disks`, RAID0 indexes (`chunk_index % n`)
        // collapse onto surviving members and a probe at logical 0x10000
        // can luck into a magic byte while every subsequent read returns
        // garbage. RAID5 with a missing data disk needs parity recovery
        // (not implemented). RAID5 with a missing parity disk would be
        // assemblable — but we still wait for completeness so the
        // interactive flow always gets the user to load every member.
        let active_members = members
            .iter()
            .filter(|m| m.sb.slot_index().is_some())
            .count() as u32;
        if active_members < raid_disks {
            continue;
        }
        let striped: Vec<StripedMember> = match collect_striped_members(&members, raid_disks) {
            Ok(m) => m,
            Err(err) => {
                diagnostics.push(format!("array {set_uuid}: {err}"));
                continue;
            }
        };
        let chunk_bytes = chunksize_sectors as u64 * 512;
        let Some(level_kind) = RaidLevel::from_raw(level, layout) else {
            // Unsupported level (RAID1 / RAID6 / RAID10 / linear) or RAID5
            // with an unrecognised layout — surface a diagnostic and leave
            // the array record without an `assembled_image_id`.
            if level == 5 {
                diagnostics.push(format!(
                    "array {set_uuid}: RAID5 layout {layout} not supported (only 0..3)"
                ));
            }
            continue;
        };
        let raid_source: Arc<dyn SectorSource + Send + Sync> = Arc::new(RaidVolume::set(
            level_kind,
            512,
            chunk_bytes,
            // Honour the SB's recorded array size when available; falls
            // back to the level-derived computation otherwise.
            Some(assembled_bytes),
            striped,
        ));
        match probe_assembled_filesystem(raid_source.as_ref()) {
            Ok(Some(fs)) => {
                let fs_kind = fs.kind_enum();
                filesystems.push(RaidFilesystem {
                    set_uuid: set_uuid.clone(),
                    kind: fs.kind_str(),
                    fsid: fs.fsid_hex,
                    total_bytes: fs.total_bytes,
                    bytes_used: fs.bytes_used,
                    num_devices: fs.num_devices,
                    sector_size: fs.sector_size,
                    node_size: fs.node_size,
                });
                assembled.push(AssembledArray {
                    set_uuid: set_uuid.clone(),
                    set_name: set_name.clone(),
                    source: raid_source,
                    size_bytes: assembled_bytes,
                    fs_kind,
                    partitions: None,
                });
            }
            Ok(None) => {}
            Err(err) => diagnostics.push(format!("filesystem probe on {set_uuid} failed: {err}")),
        }
    }

    // Pass 4 — metadata-less reconstruction. Any image not already claimed by
    // an mdadm/LVM array and carrying no RAID metadata is a candidate for a
    // hardware-RAID member set whose parameters must be recovered by brute
    // force (see `raid::heuristic`). Members already consumed above are
    // excluded so a disk is never assigned to two arrays.
    let mut consumed: std::collections::HashSet<String> = std::collections::HashSet::new();
    for array in &arrays {
        for member in &array.members {
            if let RaidMemberOrigin::Whole { image_id, .. } = &member.origin {
                consumed.insert(image_id.clone());
            }
        }
    }
    for vg in &volume_groups {
        for member in &vg.members {
            consumed.insert(member.image_id.clone());
        }
    }
    let heur = heuristic::detect(images, &consumed);
    arrays.extend(heur.arrays);
    filesystems.extend(heur.filesystems);
    assembled.extend(heur.assembled);
    diagnostics.extend(heur.diagnostics);

    let stack = RaidStack {
        images: image_summaries,
        volume_groups,
        arrays,
        filesystems,
        diagnostics,
    };
    (stack, assembled)
}
