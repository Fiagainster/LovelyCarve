import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { startImageHash, useImageHashJob, disposeImageHashJobs } from './useImageHash';
import { startBitlockerRecoveryUnlock, bitlockerJobSnapshot, disposeBitlockerJobs } from './useBitlockerUnlocks';
const ipc = vi.hoisted(() => ({ listen: vi.fn(), hash: vi.fn(), unlock: vi.fn() }));
vi.mock('../../../lib/ipc/events', () => ({ onEvent: ipc.listen }));
vi.mock('../../../lib/ipc/image', () => ({ imageHashCompute: ipc.hash }));
vi.mock('../../../lib/ipc/bitlocker', () => ({ bitlockerUnlock: ipc.unlock, bitlockerUnlockWithVmk: vi.fn(), bitlockerScanMemory: vi.fn() }));
vi.mock('../../../composables/useToast', () => ({ notifyError: vi.fn(), notifySuccess: vi.fn() }));
beforeEach(() => { vi.clearAllMocks(); ipc.listen.mockResolvedValue(vi.fn()); });
afterEach(() => { disposeImageHashJobs(); disposeBitlockerJobs(); });
describe('evidence job subscription ownership', () => {
  it('removes a failed hash listener claim so retry can start', async () => {
    ipc.listen.mockRejectedValueOnce(new Error('bridge'));
    expect(await startImageHash('image', ['md5'])).toBe(false); expect(useImageHashJob(() => 'image').value).toBeUndefined();
    ipc.hash.mockResolvedValue({md5:'digest',sha1:null,sha256:null,sm3:null,bad_regions:[],synthetic_bytes:0,read_faults_detected:false});
    expect(await startImageHash('image', ['md5'])).toBe(true); expect(ipc.hash).toHaveBeenCalledOnce();
  });
  it('removes a failed unlock listener claim instead of leaving the partition busy', async () => {
    ipc.listen.mockRejectedValueOnce(new Error('bridge'));
    expect(await startBitlockerRecoveryUnlock('image', 1, 'key')).toBe(false); expect(bitlockerJobSnapshot('image',1)).toBeUndefined();
    ipc.unlock.mockResolvedValue(undefined); expect(await startBitlockerRecoveryUnlock('image',1,'key')).toBe(true);
  });
  it('releases a hash listener attaching after main-window disposal', async () => {
    let resolve!: (stop: () => void) => void; const stop = vi.fn(); ipc.listen.mockReturnValueOnce(new Promise(yes => { resolve = yes; }));
    const running = startImageHash('image', ['md5']); disposeImageHashJobs(); resolve(stop);
    expect(await running).toBe(false); expect(stop).toHaveBeenCalledOnce(); expect(ipc.hash).not.toHaveBeenCalled();
  });
  it('releases an unlock listener attaching after main-window disposal', async () => {
    let resolve!: (stop: () => void) => void; const stop = vi.fn(); ipc.listen.mockReturnValueOnce(new Promise(yes => { resolve = yes; }));
    const running = startBitlockerRecoveryUnlock('image',1,'key'); disposeBitlockerJobs(); resolve(stop);
    expect(await running).toBe(false); expect(stop).toHaveBeenCalledOnce(); expect(ipc.unlock).not.toHaveBeenCalled();
  });
});
