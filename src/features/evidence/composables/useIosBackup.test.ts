// @vitest-environment happy-dom
import { createApp, defineComponent, h, nextTick, shallowRef, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ImageInfo } from '../../../lib/ipc/image';
import { useIosBackup } from './useIosBackup';

const rpc = vi.hoisted(() => ({ unlock: vi.fn(), audit: vi.fn(), cancel: vi.fn(), unload: vi.fn(), notify: vi.fn() }));
vi.mock('../../../lib/ipc/iosBackup', () => ({ iosBackupUnlock: rpc.unlock, iosBackupAudit: rpc.audit }));
vi.mock('../../../lib/ipc/image', () => ({ imageUnload: rpc.unload }));
vi.mock('../../../lib/ipc/taskRuntime', () => ({ taskCancel: rpc.cancel }));
vi.mock('../../../composables/useToast', () => ({ notifyError: rpc.notify }));
let app: App | undefined;
let controller: ReturnType<typeof useIosBackup>;
const current = shallowRef({ id: 'a' } as ImageInfo);
const opened = vi.fn();
beforeEach(() => {
  vi.clearAllMocks(); current.value = { id: 'a' } as ImageInfo;
  rpc.cancel.mockResolvedValue(undefined); rpc.unload.mockResolvedValue(undefined);
  app = createApp(defineComponent({ setup() { controller = useIosBackup(() => current.value, opened); return () => h('div'); } }));
  app.mount(document.createElement('div'));
});
afterEach(() => app?.unmount());

describe('iOS backup operations', () => {
  it('clears credentials, forwards the selected image and opens the child', async () => {
    let request: unknown;
    rpc.unlock.mockImplementation(async args => { request = { ...args }; return { id: 'child', path: 'decrypted://ios/a/child' }; });
    controller.password.value = 'secret';
    await controller.run('unlock');
    expect(request).toMatchObject({ imageId: 'a', password: 'secret' });
    expect(controller.password.value).toBe('');
    expect(opened).toHaveBeenCalledWith('decrypted://ios/a/child');
    expect(controller.busy.value).toBe(null);
  });

  it('supports a backup protected by an empty password', async () => {
    rpc.unlock.mockResolvedValue({ id: 'child', path: 'decrypted://ios/empty' });
    await controller.run('unlock');
    expect(rpc.unlock).toHaveBeenCalledWith(expect.objectContaining({ password: '' }));
  });

  it('surfaces wrong-password errors and allows another attempt', async () => {
    rpc.unlock.mockRejectedValue(new Error('wrong password'));
    controller.password.value = 'incorrect';
    await controller.run('unlock');
    expect(controller.error.value).toContain('wrong password');
    expect(controller.password.value).toBe('');
    expect(controller.busy.value).toBe(null);
    expect(opened).not.toHaveBeenCalled();
  });

  it('releases a late child after switching evidence A→B→A', async () => {
    let resolve!: (value: unknown) => void;
    rpc.unlock.mockReturnValue(new Promise(done => { resolve = done; }));
    const work = controller.run('unlock');
    current.value = { id: 'b' } as ImageInfo; await nextTick();
    current.value = { id: 'a' } as ImageInfo; await nextTick();
    resolve({ id: 'late', path: 'decrypted://ios/late' }); await work;
    expect(opened).not.toHaveBeenCalled();
    expect(rpc.unload).toHaveBeenCalledWith({ imageId: 'late' });
  });

  it('releases a late child after the panel is destroyed', async () => {
    let resolve!: (value: unknown) => void;
    rpc.unlock.mockReturnValue(new Promise(done => { resolve = done; }));
    const work = controller.run('unlock');
    app!.unmount(); app = undefined;
    resolve({ id: 'late', path: 'decrypted://ios/late' }); await work;
    expect(opened).not.toHaveBeenCalled();
    expect(rpc.unload).toHaveBeenCalledWith({ imageId: 'late' });
  });

  it('keeps unrated findings and drops an audit from a previous image', async () => {
    const finding = { code: 'IOS-BACKUP-DOMAIN-ABSENT', severity: null };
    rpc.audit.mockResolvedValue([finding]); await controller.run('audit');
    expect(controller.findings.value).toEqual([finding]);
    let resolve!: (value: unknown) => void;
    rpc.audit.mockReturnValue(new Promise(done => { resolve = done; }));
    const work = controller.run('audit');
    current.value = { id: 'b' } as ImageInfo; await nextTick();
    resolve([finding]); await work;
    expect(controller.findings.value).toBe(null);
  });

  it('cancels the registered task and releases a child that races with cancellation', async () => {
    let resolve!: (value: unknown) => void;
    rpc.unlock.mockReturnValue(new Promise(done => { resolve = done; }));
    const work = controller.run('unlock'); await controller.cancel();
    expect(rpc.cancel).toHaveBeenCalledWith({ taskId: rpc.unlock.mock.calls[0][0].taskId });
    resolve({ id: 'cancelled-child', path: 'decrypted://ios/cancelled' }); await work;
    expect(opened).not.toHaveBeenCalled();
    expect(rpc.unload).toHaveBeenCalledWith({ imageId: 'cancelled-child' });
  });
});
