import { effectScope } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { IpcCommandError } from '../../../lib/ipc/client';
import type { EncryptedUnlockResult } from '../../../lib/ipc/encrypted';
import type { ImageInfo } from '../../../lib/ipc/image';
import { registerEvidenceContext } from './evidenceContext';
import { disposeEncryptedVolumeUnlockJobs, registerEncryptedVolumeImageUpdater, startEncryptedVolumeUnlock } from './useEncryptedVolumeUnlocks';
import { latestUnlockedImageInfo } from './unlockedImageInfo';

const ipc = vi.hoisted(() => ({ unlock: vi.fn(), unload: vi.fn(), success: vi.fn(), error: vi.fn(), info: vi.fn() }));
vi.mock('../../tasks/api', () => ({ trackedEncryptedVolumeUnlock: ipc.unlock }));
vi.mock('../../../lib/ipc/image', () => ({ imageUnload: ipc.unload }));
vi.mock('../../../composables/useToast', () => ({ notifySuccess: ipc.success, notifyError: ipc.error, notifyInfo: ipc.info }));
const source = { id: 'source', partitions: [{ id: 2, kind: 'luks', label: 'Linux' }] } as ImageInfo;
const result = { image: { id: 'source', path: 'server.E01', unlocked_partitions: [2] }, volume_key: 'part:2', cipher: 'LUKS2 aes-xts-plain64' } as EncryptedUnlockResult;
let context = 0;
let stopContext: () => void;
let stopUpdater: () => void;
const updateImage = vi.fn();
function deferred() {
  let resolve!: (value: EncryptedUnlockResult) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<EncryptedUnlockResult>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function settle() { for (let index = 0; index < 8; index++) await Promise.resolve(); }
beforeEach(() => {
  vi.resetAllMocks();
  context = 0;
  stopContext = registerEvidenceContext(() => context);
  stopUpdater = registerEncryptedVolumeImageUpdater(updateImage);
  updateImage.mockResolvedValue(undefined);
  ipc.unload.mockResolvedValue(undefined);
});
afterEach(() => { disposeEncryptedVolumeUnlockJobs(); stopUpdater(); stopContext(); });

describe('background encrypted volume unlock ownership', () => {
  it('updates the source metadata after the initiating page stops without opening another image', async () => {
    const operation = deferred();
    ipc.unlock.mockReturnValue(operation.promise);
    const options = { scheme: 'luks' as const, password: 'secret', partitionId: 2 };
    const page = effectScope();
    expect(page.run(() => startEncryptedVolumeUnlock(source, options))).toBe(true);
    expect(ipc.unlock).toHaveBeenCalledWith({ imageId: 'source', options, taskId: expect.any(String) }, 'LUKS · 分区 2 · Linux');
    expect(updateImage).not.toHaveBeenCalled();
    page.stop();
    operation.resolve(result);
    await settle();
    expect(updateImage).toHaveBeenCalledExactlyOnceWith(result.image);
    expect(latestUnlockedImageInfo(source)).toBe(result.image);
    expect(ipc.success).toHaveBeenCalledWith('加密卷解锁成功', result.cipher);
    expect(options.password).toBe('');
  });

  it('rejects duplicate region submissions and allows retry after a failure', async () => {
    const operation = deferred();
    ipc.unlock.mockReturnValueOnce(operation.promise).mockResolvedValue(result);
    expect(startEncryptedVolumeUnlock(source, { scheme: 'luks', password: 'wrong', partitionId: 2 })).toBe(true);
    const duplicate = { scheme: 'luks' as const, password: 'duplicate', partitionId: 2 };
    expect(startEncryptedVolumeUnlock(source, duplicate)).toBe(false);
    expect(duplicate.password).toBe('');
    expect(ipc.unlock).toHaveBeenCalledOnce();
    operation.reject(new Error('authentication failed'));
    await settle();
    expect(ipc.error).toHaveBeenLastCalledWith('解锁加密容器失败', 'Error: authentication failed');
    expect(startEncryptedVolumeUnlock(source, { scheme: 'luks', password: 'correct', partitionId: 2 })).toBe(true);
    await settle();
    expect(updateImage).toHaveBeenCalledOnce();
  });

  it('ignores a late completion after an A → B → A case switch without unloading the source', async () => {
    const operation = deferred();
    ipc.unlock.mockReturnValue(operation.promise);
    startEncryptedVolumeUnlock(source, { scheme: 'luks', password: 'secret' });
    context += 2;
    operation.resolve(result);
    await settle();
    expect(ipc.unload).not.toHaveBeenCalled();
    expect(updateImage).not.toHaveBeenCalled();
    expect(ipc.success).not.toHaveBeenCalled();
  });

  it('ignores results after main-window disposal and suppresses cancellation errors', async () => {
    const operation = deferred();
    ipc.unlock.mockReturnValueOnce(operation.promise);
    startEncryptedVolumeUnlock(source, { scheme: 'luks', password: 'secret' });
    disposeEncryptedVolumeUnlockJobs();
    operation.resolve(result);
    await settle();
    expect(ipc.unload).not.toHaveBeenCalled();
    ipc.unlock.mockRejectedValue(new IpcCommandError({ code: 'cancelled', message: 'cancelled', context: null, retryable: false }));
    startEncryptedVolumeUnlock(source, { scheme: 'luks', password: 'secret' });
    await settle();
    expect(ipc.error).not.toHaveBeenCalled();
    expect(updateImage).not.toHaveBeenCalled();
  });
});
