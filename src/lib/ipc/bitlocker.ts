import { invokeCmd } from './client';



export function bitlockerStatus(args: { imageId: string; partitionId: number }): Promise<{ locked: boolean }> {
  return invokeCmd<{ locked: boolean }>('bitlocker_status', args);
}

export function bitlockerUnlock(args: {
  imageId: string;
  partitionId: number;
  taskId: string;
  recoveryPassword?: string | null;
  password?: string | null;
}): Promise<void> {
  return invokeCmd<void>('bitlocker_unlock', args);
}

export function bitlockerUnlockWithVmk(args: {
  imageId: string;
  partitionId: number;
  taskId: string;
  vmkHex: string;
}): Promise<void> {
  return invokeCmd<void>('bitlocker_unlock_with_vmk', args);
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
