export type BitlockerUnlockMode = 'password' | 'recovery' | 'vmk';

export interface BitlockerUnlockJob {
  imageId: string;
  partitionId: number;
  taskId: string;
  mode: BitlockerUnlockMode;
  bytesDone: number;
  bytesTotal: number;
  cancelling: boolean;
}

export interface BitlockerScanJob {
  imageId: string;
  partitionId: number;
  taskId: string;
  bytesDone: number;
  bytesTotal: number;
  cancelling: boolean;
}

export interface BitlockerVmkCandidate {
  offset: number;
  type_flag: number;
  key_type: number;
  vmk_hex: string;
  recovery_key: string | null;
  already_tried: boolean;
}

export interface BitlockerVmkScanResult {
  partition_id: number;
  candidates: BitlockerVmkCandidate[];
}

export type BitlockerInputMode = 'password' | 'recovery' | 'memory';

function formatBitlockerBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(2)} ${units[i]}`;
}

/** UI projection shared by unlock and memory scan jobs; does not own task state. */
export function bitlockerProgress(job: { bytesDone: number; bytesTotal: number } | undefined) {
  const percent = !job || job.bytesTotal === 0 ? 0 : Math.min(100, (job.bytesDone / job.bytesTotal) * 100);
  return { percent, percentText: `${percent.toFixed(1)}%`, bytesText: !job ? '' : job.bytesTotal === 0 ? '准备中…' : `${formatBitlockerBytes(job.bytesDone)} / ${formatBitlockerBytes(job.bytesTotal)}` };
}
