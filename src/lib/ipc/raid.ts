import { invokeCmd } from './client';



export interface RaidStack {
  images: RaidImage[];
  volume_groups: RaidVolumeGroup[];
  arrays: RaidArray[];
  filesystems: RaidFilesystem[];
  diagnostics: string[];
}

export interface RaidImage {
  image_id: string;
  path: string;
  size_bytes: number;
  partition_count: number;
  lvm_pv_count: number;
}

export interface RaidVolumeGroup {
  vg_uuid: string;
  vg_name: string;
  extent_size_sectors: number;
  members: RaidVgMember[];
  logical_volumes: RaidLogicalVolume[];
}

export interface RaidVgMember {
  image_id: string;
  partition_id: number;
  pv_local_id: string;
  pv_uuid: string;
  pe_start_sectors: number;
  partition_byte_offset: number;
  partition_byte_length: number;
}

export interface RaidLogicalVolume {
  name: string;
  size_bytes: number;
  segments: RaidLvSegment[];
}

export interface RaidLvSegment {
  pv_local_id: string;
  image_id: string;
  partition_id: number;
  byte_offset: number;
  byte_length: number;
}

export interface RaidArray {
  set_uuid: string;
  set_name: string;
  /** `0` for RAID0, `1` for RAID1, `5` for RAID5, … */
  level: number;
  /** Raw mdadm v1.x `layout` field — only meaningful for RAID5/6. */
  layout: number;
  chunksize_sectors: number;
  raid_disks: number;
  data_offset_sectors: number;
  assembled_bytes: number;
  members: RaidArrayMember[];
  /** Set once the backend has registered the assembled array as a synthetic
   *  `DiskImage`; null for incomplete sets, unsupported levels, or when no
   *  recognised filesystem was found on the assembled device. */
  assembled_image_id: string | null;
  /** `true` when this array was reconstructed by the metadata-less heuristic
   *  (no mdadm superblock / LVM label) — its level, chunk, layout and disk
   *  order were *recovered* by brute force and verified against the
   *  filesystem, not read from on-disk metadata. */
  heuristic: boolean;
}

export interface RaidArrayMember {
  dev_number: number;
  /** Slot within the array (0..raid_disks-1); null for spare/faulty. */
  slot: number | null;
  device_uuid: string;
  data_size_sectors: number;
  /** `true` when this column's payload was rebuilt from parity rather than read
   *  off the disk — i.e. a dropped/stale RAID5 member the heuristic recovered.
   *  Always `false` for mdadm-assembled members. Drives the dropped-disk styling
   *  in the layout diagram. */
  reconstructed: boolean;
  origin: RaidMemberOrigin;
}

export type RaidMemberOrigin =
  | { kind: 'lv'; vg_uuid: string; vg_name: string; lv_name: string; size_bytes: number }
  | { kind: 'whole'; image_id: string; partition_id: number; byte_offset: number; byte_length: number };

export interface RaidFilesystem {
  set_uuid: string;
  kind: string;
  fsid: string | null;
  total_bytes: number | null;
  bytes_used: number | null;
  num_devices: number | null;
  sector_size: number | null;
  node_size: number | null;
}

/** Run LVM → mdadm → filesystem detection over every loaded image. Idempotent:
 *  successfully assembled arrays are registered as synthetic images once and
 *  reused on re-runs. Heavy-ish (reads member superblocks) but bounded. */
export function raidAutoDetect(): Promise<RaidStack> {
  return invokeCmd<RaidStack>('raid_auto_detect');
}

/** Explicit geometry for a manual RAID reconstruction (the builder fallback).
 *  `member_image_ids` are loaded-image ids in column (slot) order. */
export interface ManualRaidParams {
  member_image_ids: string[];
  /** `0` (striped) or `5` (single parity). */
  level: number;
  /** RAID5 layout 0..3 (left/right × asymmetric/symmetric); ignored for RAID0. */
  layout: number;
  /** Stripe/chunk size in 512-byte sectors. */
  chunk_sectors: number;
  /** Per-member reserved-header size in 512-byte sectors. */
  data_offset_sectors: number;
  /** Column (slot) of a dropped member to rebuild from parity, or null. */
  missing_slot: number | null;
}

export interface ManualRaidPartition {
  kind: string;
  start_lba: number;
  length_lba: number;
  label: string | null;
}

/** Result of `raid_manual_assemble`: the registered synthetic image id, its
 *  logical size, and a preview of the partitions the geometry yields (empty
 *  when the parameters produce no recognisable table). */
export interface ManualRaidResult {
  image_id: string;
  size_bytes: number;
  partitions: ManualRaidPartition[];
}

/** Assemble a RAID device from an explicit, examiner-supplied geometry and
 *  register it as a synthetic image. Idempotent on the geometry. */
export function raidManualAssemble(params: ManualRaidParams): Promise<ManualRaidResult> {
  return invokeCmd<ManualRaidResult>('raid_manual_assemble', { params });
}
