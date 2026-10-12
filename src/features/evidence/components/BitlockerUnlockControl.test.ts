// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, h, nextTick, ref, type App } from 'vue';
import type { BitlockerScanJob, BitlockerUnlockJob, BitlockerVmkScanResult } from '../model/bitlocker';
import BitlockerUnlockControl from './BitlockerUnlockControl.vue';

const calls = vi.hoisted(() => ({ status: vi.fn(), reminder: vi.fn(), password: vi.fn(), recovery: vi.fn(), scan: vi.fn(), vmk: vi.fn(), cancelUnlock: vi.fn(), cancelScan: vi.fn(), open: vi.fn() }));
const job = ref<BitlockerUnlockJob>(); const scanJob = ref<BitlockerScanJob>();
vi.mock('../composables/useFileSession', () => ({ dismissBitlockerReminder: calls.reminder }));
vi.mock('../composables/useBitlockerUnlocks', () => ({ useBitlockerJob: () => job, useBitlockerScanJob: () => scanJob, startBitlockerPasswordUnlock: calls.password, startBitlockerRecoveryUnlock: calls.recovery, startBitlockerMemoryScan: calls.scan, startBitlockerVmkUnlock: calls.vmk, cancelBitlockerUnlock: calls.cancelUnlock, cancelBitlockerMemoryScan: calls.cancelScan }));
vi.mock('../composables/useImageExport', () => ({ useImageExport: () => ({ jobFor: () => undefined, startBitlockerDecryptedImageExport: vi.fn() }) }));
vi.mock('../../../lib/ipc/bitlocker', () => ({ bitlockerStatus: calls.status }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: calls.open }));

let app: App; let host: HTMLDivElement;
const imageId = ref('a'); const unlocked = vi.fn();
const partition = { id: 1, kind: 'bitlocker', label: null, start_lba: 0, length_lba: 10 };
const key = '031174-056914-397793-502348-055847-196306-284306-262174';
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { resolve, promise }; }
async function settle() { for (let index = 0; index < 10; index++) await nextTick(); }
async function mount() { host = document.createElement('div'); document.body.append(host); app = createApp({ render: () => h(BitlockerUnlockControl, { imageId: imageId.value, partition, onUnlocked: unlocked }) }); app.mount(host); await settle(); }
async function open() {
  // A native click occurs after Vue's listener attachment timestamp.
  await new Promise(resolve => setTimeout(resolve, 1));
  host.querySelector<HTMLButtonElement>('.bde-btn')!.click(); await settle();
  expect(document.querySelector('.bde-popover')).not.toBeNull();
}
function popup() { return document.querySelector<HTMLElement>('.bde-popover')!; }
function button(label: string) { return [...popup().querySelectorAll<HTMLButtonElement>('button')].find(candidate => candidate.textContent?.trim() === label)!; }
async function click(label: string) {
  // Give newly rendered Vue listeners a later native event timestamp before bubbling.
  await new Promise(resolve => setTimeout(resolve, 1));
  button(label).click(); await settle();
}
function input(value: string) { const field = popup().querySelector<HTMLInputElement>('input')!; field.value = value; field.dispatchEvent(new Event('input', { bubbles: true })); return field; }
beforeEach(() => { vi.resetAllMocks(); imageId.value = 'a'; job.value = undefined; scanJob.value = undefined; calls.status.mockResolvedValue({ locked: true, encryption_method: null }); });
afterEach(() => { app?.unmount(); host?.remove(); vi.unstubAllGlobals(); });

describe('BitLocker control composition', () => {
  it('focuses the recovery field, links validation feedback and submits the normalized key', async () => {
    await mount(); await open(); const field = input('031174-0569'); await settle();
    expect(document.activeElement).toBe(field); expect(field.getAttribute('aria-invalid')).toBeNull();
    field.dispatchEvent(new Event('blur')); await settle();
    expect(field.getAttribute('aria-invalid')).toBe('true');
    expect(document.getElementById(field.getAttribute('aria-describedby')!)?.textContent).toContain('只有 4 位');
    input(key.replaceAll('-', ' ')); await settle(); calls.recovery.mockResolvedValue(true);
    await click('解锁'); await settle();
    expect(calls.recovery).toHaveBeenCalledWith('a', 1, key); expect(document.querySelector('.bde-popover')).toBeNull();
  });
  it('switches input focus and emits the unlocked partition after password success', async () => {
    calls.status.mockResolvedValueOnce({ locked: true }).mockResolvedValue({ locked: false, encryption_method: 'AES' });
    calls.password.mockResolvedValue(true); await mount(); await open(); await click('密码'); await settle();
    const field = input('secret'); await settle(); expect(document.activeElement).toBe(field);
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await settle();
    expect(calls.password).toHaveBeenCalledWith('a', 1, 'secret'); expect(unlocked).toHaveBeenCalledWith(1);
    expect(host.querySelector('.bde-status')?.textContent).toContain('UNLOCKED');
  });
  it('keeps memory scanning alive after closing the popup and displays the returned candidates on reopen', async () => {
    const scan = deferred<BitlockerVmkScanResult>(); calls.open.mockResolvedValue('D:/memory.raw'); calls.scan.mockReturnValue(scan.promise);
    await mount(); await open(); await click('内存镜像'); await settle(); await click('选择内存镜像'); await settle(); await click('开始扫描');
    scanJob.value = { imageId: 'a', partitionId: 1, taskId: 'scan', bytesDone: 50, bytesTotal: 100, cancelling: false }; await settle();
    expect(popup().textContent).toContain('50.0%');
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })); await settle();
    expect(document.querySelector('.bde-popover')).toBeNull(); expect(calls.cancelScan).not.toHaveBeenCalled();
    scan.resolve({ partition_id: 1, candidates: [{ offset: 1, type_flag: 0, key_type: 0, vmk_hex: 'AA', recovery_key: key, already_tried: true }] }); scanJob.value = undefined; await settle(); await open();
    expect(popup().textContent).toContain(key); expect(popup().querySelector<HTMLButtonElement>('.bde-mem-row')?.disabled).toBe(true);
  });
  it('does not publish a stale A→B→A lock status or old folder selection', async () => {
    const status = deferred<{ locked: boolean }>(); calls.status.mockReturnValueOnce(status.promise);
    const selected = deferred<string>(); calls.open.mockReturnValue(selected.promise);
    await mount(); await open(); await click('内存镜像'); await settle(); await click('选择内存镜像');
    imageId.value = 'b'; await settle(); imageId.value = 'a'; await settle(); status.resolve({ locked: false }); selected.resolve('D:/old.raw'); await settle();
    expect(host.querySelector('.bde-status')?.textContent?.trim()).toBe('LOCKED'); expect(unlocked).not.toHaveBeenCalled();
    await open(); expect(popup().querySelector('.bde-file-path')).toBeNull();
  });
  it('ignores IME Escape and cancels queued positioning when the control is destroyed', async () => {
    const raf = vi.fn(() => 17); const cancel = vi.fn(); vi.stubGlobal('requestAnimationFrame', raf); vi.stubGlobal('cancelAnimationFrame', cancel);
    await mount(); await open(); document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', isComposing: true })); await settle();
    expect(document.querySelector('.bde-popover')).not.toBeNull();
    window.dispatchEvent(new Event('scroll')); expect(raf).toHaveBeenCalledOnce(); app.unmount();
    expect(cancel).toHaveBeenCalledWith(17); expect(document.querySelector('.bde-popover')).toBeNull();
    expect(calls.cancelUnlock).not.toHaveBeenCalled(); expect(calls.cancelScan).not.toHaveBeenCalled();
  });
  it('does not apply an old unlock completion to the new partition view', async () => {
    const unlock = deferred<boolean>(); calls.password.mockReturnValue(unlock.promise);
    await mount(); await open(); await click('密码'); await settle(); input('old-password'); await settle(); await click('解锁');
    imageId.value = 'b'; await settle(); await open(); const newField = input('new-password'); await settle();
    const statusCalls = calls.status.mock.calls.length; unlock.resolve(true); await settle();
    expect(calls.password).toHaveBeenCalledWith('a', 1, 'old-password'); expect(calls.status).toHaveBeenCalledTimes(statusCalls);
    expect(newField.value).toBe('new-password'); expect(document.querySelector('.bde-popover')).not.toBeNull();
  });
  it('ignores a memory scan response after the control is destroyed', async () => {
    const scan = deferred<BitlockerVmkScanResult>(); calls.open.mockResolvedValue('D:/memory.raw'); calls.scan.mockReturnValue(scan.promise);
    await mount(); await open(); await click('内存镜像'); await settle(); await click('选择内存镜像'); await settle(); await click('开始扫描');
    app.unmount(); scan.resolve({ partition_id: 1, candidates: [] }); await settle();
    expect(document.querySelector('.bde-popover')).toBeNull(); expect(calls.cancelScan).not.toHaveBeenCalled();
  });
});
