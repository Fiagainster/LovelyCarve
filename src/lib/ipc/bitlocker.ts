import { invokeCmd } from './client';



export interface BitlockerStatus {
  locked: boolean;
  size_bytes: number | null;
  encryption_method: string | null;
  cached_dump: string | null;
}

export interface BitlockerMetadata {
  version: number;
  bytes_per_sector: number;
  volume_guid: string;
  creation_time: string;
  description: string | null;
  encryption_method: number;
  protector_types: number[];
  key_protectors: { identifier: string; protection_type: number }[];
  encrypted_volume_size: number;
  used_space_only: boolean;
}

export function bitlockerMetadata(args: { imageId: string; partitionId: number }): Promise<BitlockerMetadata> {
  return invokeCmd<BitlockerMetadata>('bitlocker_metadata', args);
}

export function bitlockerStatus(args: { imageId: string; partitionId: number }): Promise<BitlockerStatus> {
  return invokeCmd<BitlockerStatus>('bitlocker_status', args);
}

export function bitlockerUnlock(args: {
  imageId: string;
  partitionId: number;
  taskId: string;
  recoveryPassword?: string | null;
  password?: string | null;
  startupKeyPath?: string | null;
  fvekHex?: string | null;
  tweakHex?: string | null;
}): Promise<BitlockerStatus> {
  return invokeCmd<BitlockerStatus>('bitlocker_unlock', args);
}

export function bitlockerUnlockWithVmk(args: {
  imageId: string;
  partitionId: number;
  taskId: string;
  vmkHex: string;
}): Promise<{ status: BitlockerStatus; recovery_key: string }> {
  return invokeCmd<{ status: BitlockerStatus; recovery_key: string }>('bitlocker_unlock_with_vmk', args);
}

export function bitlockerScanMemory<T>(args: {
  imageId: string;
  partitionId: number;
  memoryPath: string;
  taskId: string;
}): Promise<T> {
  return invokeCmd<T>('bitlocker_scan_memory', args);
}

/** Export the full loaded image as RAW while replacing the selected, already
 *  unlocked BitLocker partition with its decrypted plaintext bytes. Other
 *  sectors, including the partition table and unrelated partitions, are copied
 *  unchanged. Emits {@link IMAGE_EXPORT_PROGRESS_EVENT}. */
export function bitlockerExportDecryptedImage(args: {
  imageId: string;
  partitionId: number;
  destination: string;
  taskId: string;
}): Promise<number> {
  return invokeCmd<number>('bitlocker_export_decrypted_image', args);
}
