import { notifyError, notifyInfo, notifySuccess } from '../../../composables/useToast';
import type { EncryptedUnlockOptions } from '../../../lib/ipc/encrypted';
import type { ImageInfo } from '../../../lib/ipc/image';
import { IpcCommandError } from '../../../lib/ipc/client';
import { trackedEncryptedVolumeUnlock } from '../../tasks/api';
import { evidenceContext } from './evidenceContext';
import { clearUnlockedImageInfo, rememberUnlockedImageInfo } from './unlockedImageInfo';

// The main window owns completion and source metadata, independently of the dialog.
const jobs = new Map<string, string>();
let generation = 0;
let updateImage: ((info: ImageInfo) => void) | null = null;

export function registerEncryptedVolumeImageUpdater(updater: (info: ImageInfo) => void) {
  updateImage = updater;
  return () => { if (updateImage === updater) updateImage = null; };
}

export function disposeEncryptedVolumeUnlockJobs() {
  generation++;
  jobs.clear();
  clearUnlockedImageInfo();
}

/** Submit immediately; task tracking and metadata updates outlive the page. */
export function startEncryptedVolumeUnlock(info: ImageInfo, options: EncryptedUnlockOptions): boolean {
  const key = `${info.id}:${options.partitionId ?? 'whole'}`;
  if (jobs.has(key)) {
    options.password = '';
    options.masterKeyHex = undefined;
    notifyError('解锁加密容器失败', '此加密区域已有解锁任务，可在任务中心查看或取消');
    return false;
  }
  const taskId = crypto.randomUUID();
  const epoch = generation;
  const context = evidenceContext();
  const updater = updateImage;
  const current = () => epoch === generation && context === evidenceContext();
  const partition = info.partitions.find(partition => partition.id === options.partitionId);
  const subject = `${options.scheme === 'luks' ? 'LUKS' : 'VeraCrypt'} · ${partition ? `分区 ${partition.id}${partition.label ? ` · ${partition.label}` : ''}` : '整个镜像 / 容器'}`;
  jobs.set(key, taskId);
  void (async () => {
    try {
      options.target = 'sourcePartition';
      const result = await trackedEncryptedVolumeUnlock({ imageId: info.id, options, taskId }, subject);
      if (!current()) return;
      rememberUnlockedImageInfo(result.image);
      updater?.(result.image);
      notifySuccess('加密卷解锁成功', result.cipher);
    } catch (error) {
      if (current() && !(error instanceof IpcCommandError && error.code === 'cancelled')) {
        notifyError('解锁加密容器失败', String(error));
      } else if (!current()) {
        console.warn('旧会话加密卷解锁已结束', error);
      }
    } finally {
      options.password = '';
      options.masterKeyHex = undefined;
      if (jobs.get(key) === taskId) jobs.delete(key);
    }
  })();
  notifyInfo('已开始后台解锁', '可在任务中心查看状态或取消');
  return true;
}
