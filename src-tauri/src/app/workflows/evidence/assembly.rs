//! raid implementation.
use super::*;
use crate::core::evidence::image::DiskImage;
use crate::core::filesystem::evidence_session::EvidenceSession;

/// Walk every loaded image, parse LVM PV metadata, join PVs across images
/// by VG UUID, materialise LVs as virtual block devices, scan LV offset
/// 0x1000 for mdadm v1.2 superblocks, group members by `set_uuid`, and
/// finally probe the assembled RAID0 device for a btrfs primary superblock.
///
/// For each successfully assembled RAID0, this also registers the result
/// as a synthetic `DiskImage` in `state.images` so it shows up in the FS
/// tree, hex view, and image switcher just like a regular image.
/// Idempotent: re-running on the same image set reuses the previously
/// registered synthetic image instead of allocating a duplicate.
pub async fn raid_auto_detect(state: State<'_, AppState>) -> Result<RaidStack, AppError> {
    // Snapshot only "real" loaded images — feeding a previously assembled
    // synthetic image back into detection would be double-counting and
    // could even infinite-loop when LVM PV labels happen to match. A shadow
    // copy repeats its parent volume, so it would pose as a mirror member.
    let snapshot: Vec<_> = {
        let images = state.images.lock().await;
        images
            .values()
            .filter(|img| {
                !img.path.to_string_lossy().starts_with("assembled://")
                    && !matches!(img.derived, Some(DerivedImage::ShadowCopy { .. }))
            })
            .map(|image| Arc::clone(image.disk_image()))
            .collect()
    };
    let (mut stack, assembled) =
        tokio::task::spawn_blocking(move || raid::detect_with_sources(&snapshot))
            .await
            .map_err(|err| AppError::task_panicked("raid_auto_detect", err))?;

    // Phase 1: snapshot the idempotency cache and the set of live image ids
    // WITHOUT holding either lock across the CPU-bound DiskImage::from_assembled
    // build below. Holding `assembled_arrays` + `images` together across
    // registration was a deadlock against `image_list` (which locks
    // images→assembled, the opposite order) and starved concurrent
    // image_load/unload on the heavy from_assembled call. Cloning a handful of
    // short ids here is negligible by comparison.
    let cache_snapshot: std::collections::HashMap<String, String> = {
        let cache = state.assembled_arrays.lock().await;
        cache.clone()
    };
    let live_image_ids: std::collections::HashSet<String> = {
        let images = state.images.lock().await;
        images.keys().cloned().collect()
    };
    // Evidence ids recorded for `assembled://` paths in the active case.
    // Re-registering an array in a fresh session must converge on the id the
    // case already knows — evidence entries and persisted volume snapshots
    // both key on image_id, so a fresh uuid per session would pile up
    // duplicate evidence rows and miss every snapshot (the same stability
    // contract `image_load` honours for real images). Known gap: a synthetic
    // registered while NO case was active keeps its session uuid on the
    // cache-hit path above, so opening a case afterwards records a second id
    // for the same path — mirroring the pre-existing behaviour for real
    // images loaded before a case is opened.
    let evidence_ids_by_path: std::collections::HashMap<String, String> = {
        let case = state.active_case.lock().await;
        case.as_ref()
            .map(|case| {
                case.evidence_refs()
                    .into_iter()
                    .map(|evidence| (evidence.path, evidence.image_id))
                    .collect()
            })
            .unwrap_or_default()
    };

    // Built synthetic images awaiting registration, paired with the array
    // slot they belong to. Construction (from_assembled) happens here,
    // lock-free.
    struct PendingRegistration {
        array_index: usize,
        cache_key: String,
        image: DiskImage,
    }
    let mut pending: Vec<PendingRegistration> = Vec::new();
    // Ids already adopted by an earlier array in THIS loop — `live_image_ids`
    // is a pre-loop snapshot and can't see them.
    let mut adopted_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (array_index, array) in stack.arrays.iter_mut().enumerate() {
        let Some(materialised) = assembled.iter().find(|a| a.set_uuid == array.set_uuid) else {
            continue;
        };
        let cache_key = array.set_uuid.clone();
        if let Some(existing) = cache_snapshot.get(&cache_key) {
            if live_image_ids.contains(existing) {
                array.assembled_image_id = Some(existing.clone());
                continue;
            }
        }
        // The label doubles as the synthetic image's `assembled://<label>`
        // path, which case evidence AND `image_load`'s by-path reuse key on —
        // so it must be injective per array. The full set_uuid (not a
        // prefix) is required: set_uuid is read verbatim from on-disk
        // superblocks, so crafted evidence can collide any shorter prefix,
        // while two *registered* arrays can never share a full set_uuid
        // (detection groups members by it).
        let label = if materialised.set_name.is_empty() {
            format!("md:{}", materialised.set_uuid)
        } else {
            format!("md:{}-{}", materialised.set_name, materialised.set_uuid)
        };
        let sector_size = 512_usize;
        let length_lba = materialised.size_bytes / sector_size as u64;
        // A metadata-less hardware-RAID array carries its own partition table
        // (MBR/GPT with several filesystems); the mdadm/LVM path leaves it
        // `None` and the array is a single bare filesystem on the md device.
        let partitions = match &materialised.partitions {
            Some(parts) => parts.clone(),
            None => vec![Partition {
                id: 0,
                kind: materialised.fs_kind,
                start_lba: 0,
                length_lba,
                label: Some(raid::synthetic_partition_label(materialised)),
                unique_guid: None,
            }],
        };
        let mut synthetic =
            DiskImage::from_assembled(label, materialised.source.clone(), partitions, sector_size)
                .map_err(|err| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("registering assembled array failed: {err:#}"),
                    )
                })?;
        // Adopt the case-recorded id unless something live already owns it
        // or an earlier array in this loop claimed it (then the fresh uuid
        // stands — never overwrite a loaded image's slot).
        if let Some(stable_id) = evidence_ids_by_path.get(synthetic.path.to_string_lossy().as_ref())
        {
            if !live_image_ids.contains(stable_id) && adopted_ids.insert(stable_id.clone()) {
                synthetic.id = stable_id.clone();
            }
        }
        pending.push(PendingRegistration {
            array_index,
            cache_key,
            image: synthetic,
        });
    }

    // Phase 2: publish freshly built images. Lock images→assembled (the same
    // order `image_list` uses) and hold both only for cheap map inserts —
    // never across from_assembled. Re-check the cache under lock so two
    // concurrent detect calls converge on one synthetic image per set_uuid
    // instead of leaking duplicates.
    if !pending.is_empty() {
        let mut images = state.images.lock().await;
        let mut cache = state.assembled_arrays.lock().await;
        for PendingRegistration {
            array_index,
            cache_key,
            mut image,
        } in pending
        {
            if let Some(existing) = cache.get(&cache_key) {
                if images.contains_key(existing) {
                    stack.arrays[array_index].assembled_image_id = Some(existing.clone());
                    continue;
                }
                cache.remove(&cache_key);
            }
            // Re-validate the adopted id under the lock: a concurrent detect
            // or load may have registered it since the phase-1 snapshot.
            // Never overwrite a live slot — mint a fresh uuid instead.
            if images.contains_key(&image.id) {
                image.id = uuid::Uuid::new_v4().to_string();
            }
            let image_id = image.id.clone();
            images.insert(image_id.clone(), Arc::new(EvidenceSession::from(image)));
            cache.insert(cache_key, image_id.clone());
            stack.arrays[array_index].assembled_image_id = Some(image_id);
        }
    }

    Ok(stack)
}

/// Assemble a RAID device from an explicit, user-supplied geometry (the manual
/// "RAID builder" fallback for arrays the automatic heuristic cannot crack) and
/// register it as a synthetic image so the rest of the app can browse / export
/// it. Idempotent on the geometry: re-running with the same parameters reuses
/// the previously registered synthetic image.
pub async fn raid_manual_assemble(
    state: State<'_, AppState>,
    params: raid::ManualRaidParams,
) -> Result<ManualRaidResult, AppError> {
    let images: Vec<Arc<DiskImage>> = {
        let imgs = state.images.lock().await;
        imgs.values()
            .map(|image| Arc::clone(image.disk_image()))
            .collect()
    };
    let array = tokio::task::spawn_blocking(move || raid::manual_assemble(&images, &params))
        .await
        .map_err(|err| AppError::task_panicked("raid_manual_assemble", err))?
        .map_err(AppError::from)?;

    let partitions: Vec<ManualPartitionInfo> = array
        .partitions
        .as_ref()
        .map(|ps| {
            ps.iter()
                .map(|p| ManualPartitionInfo {
                    kind: raid::kind_label(p.kind).to_string(),
                    start_lba: p.start_lba,
                    length_lba: p.length_lba,
                    label: p.label.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    let image_id = register_assembled_image(&state, &array).await?;
    Ok(ManualRaidResult {
        image_id,
        size_bytes: array.size_bytes,
        partitions,
    })
}

/// Register an [`raid::AssembledArray`] as a synthetic `EvidenceSession`, reusing an
/// existing one with the same `assembled://` path when present. A streamlined
/// sibling of the `raid_auto_detect` registration (no case-evidence id
/// stability — manual builds are transient and user-initiated).
async fn register_assembled_image(
    state: &State<'_, AppState>,
    array: &raid::AssembledArray,
) -> Result<String, AppError> {
    let label = if array.set_name.is_empty() {
        format!("md:{}", array.set_uuid)
    } else {
        format!("md:{}-{}", array.set_name, array.set_uuid)
    };
    let path_str = format!("assembled://{label}");
    {
        let images = state.images.lock().await;
        if let Some((id, _)) = images
            .iter()
            .find(|(_, im)| im.path.to_string_lossy() == path_str)
        {
            return Ok(id.clone());
        }
    }
    let length_lba = array.size_bytes / 512;
    let partitions = match &array.partitions {
        Some(parts) => parts.clone(),
        None => vec![Partition {
            id: 0,
            kind: array.fs_kind,
            start_lba: 0,
            length_lba,
            label: Some(raid::synthetic_partition_label(array)),
            unique_guid: None,
        }],
    };
    let synthetic = DiskImage::from_assembled(label, array.source.clone(), partitions, 512)
        .map_err(|err| {
            AppError::new(
                ErrorCode::Internal,
                format!("registering assembled array failed: {err:#}"),
            )
        })?;
    let id = synthetic.id.clone();
    let mut images = state.images.lock().await;
    images.insert(id.clone(), Arc::new(EvidenceSession::from(synthetic)));
    Ok(id)
}
