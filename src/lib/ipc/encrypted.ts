import { invokeCmd } from './client';
import type { ImageInfo } from './image';

export interface EncryptedUnlockOptions {
  target?: 'newImage' | 'sourcePartition';
  scheme: 'luks' | 'veracrypt';
  password?: string;
  masterKeyHex?: string;
  partitionId?: number;
  offset?: number;
  length?: number;
  hidden?: boolean;
  pim?: number;
}

export interface EncryptedUnlockResult {
  volume_key?: string;
  image: ImageInfo;
  scheme: 'luks' | 'veracrypt';
  cipher: string;
  encryption_sector_size: number;
  source_offset: number;
  source_length: number;
  hidden: boolean;
}

export function encryptedVolumeUnlock(args: {
  imageId: string;
  options: EncryptedUnlockOptions;
  taskId?: string;
}): Promise<EncryptedUnlockResult> {
  return invokeCmd<EncryptedUnlockResult>('encrypted_volume_unlock', args);
}
