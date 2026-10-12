// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, ref } from 'vue';
import { useImageRecognition } from './useImageRecognition';
import type { QrScanOutcome, QrScanProgress } from '../../../lib/ipc/qrcode';
import type { OcrOutcome } from '../../../lib/ipc/ocr';

const mocks = vi.hoisted(() => ({ qr: vi.fn(), ocr: vi.fn(), listen: vi.fn(), cancel: vi.fn(), error: vi.fn() }));
vi.mock('../../../lib/ipc/qrcode', () => ({ qrScan: mocks.qr }));
vi.mock('../../../lib/ipc/ocr', () => ({ ocrScan: mocks.ocr }));
vi.mock('../../../lib/ipc/events', () => ({ onEvent: mocks.listen }));
vi.mock('../../../lib/ipc/taskRuntime', () => ({ taskCancel: mocks.cancel }));
vi.mock('../../../composables/useToast', () => ({ notifyError: mocks.error }));
const scopes: ReturnType<typeof effectScope>[] = [];
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes; }); return { promise, resolve }; }
const qr = (text: string): QrScanOutcome => ({ codes: [{ text, points: [], transform: 'original' }], width: 100, height: 100, attempts: 1, cancelled: false, detector: 'wechat' });
const ocr = (text: string): OcrOutcome => ({ regions: [{ text, confidence: 1, points: [[0, 0], [10, 0], [10, 10]] }], width: 100, height: 100, count: 1 });
function setup() {
  const scope = effectScope(); scopes.push(scope);
  const image = ref('image-a'), node = ref('file-a');
  const recognition = scope.run(() => useImageRecognition({ imageId: () => image.value, nodeId: () => node.value, category: () => 'image' }))!;
  return { recognition, image, node, scope };
}
beforeEach(() => {
  vi.clearAllMocks(); mocks.listen.mockImplementation(async () => vi.fn());
  mocks.qr.mockResolvedValue(qr('current')); mocks.ocr.mockResolvedValue(ocr('current')); mocks.cancel.mockResolvedValue(undefined);
});
afterEach(() => { for (const scope of scopes.splice(0)) scope.stop(); vi.useRealTimers(); vi.restoreAllMocks(); });

describe('image recognition task ownership', () => {
  it('releases a listener arriving after disposal without starting a scan', async () => {
    const pending = deferred<() => void>(), unlisten = vi.fn(); mocks.listen.mockReturnValueOnce(pending.promise);
    const state = setup(); const operation = state.recognition.runQrScan(); state.scope.stop(); pending.resolve(unlisten); await operation;
    expect(unlisten).toHaveBeenCalledOnce(); expect(mocks.qr).not.toHaveBeenCalled();
  });
  it('captures the evidence before waiting for a listener and ignores replaced subscriptions', async () => {
    const pending = deferred<() => void>(), unlisten = vi.fn(); mocks.listen.mockReturnValueOnce(pending.promise);
    const state = setup(); const first = state.recognition.runQrScan(); state.image.value = 'image-b'; state.recognition.resetImageRecognition();
    await state.recognition.runQrScan(); pending.resolve(unlisten); await first;
    expect(unlisten).toHaveBeenCalledOnce();
    expect(mocks.qr.mock.calls).toEqual([[expect.objectContaining({ imageId: 'image-b', nodeId: 'file-a' })]]);
    expect(state.recognition.qrResult.value).toEqual(qr('current'));
  });
  it('cancels a running old QR scan and does not let its result/finally clear a new scan', async () => {
    const old = deferred<QrScanOutcome>(), recent = deferred<QrScanOutcome>(); mocks.qr.mockReturnValueOnce(old.promise).mockReturnValueOnce(recent.promise);
    const state = setup(); const first = state.recognition.runQrScan(); await Promise.resolve();
    const oldTask = mocks.qr.mock.calls[0][0].taskId;
    state.node.value = 'file-b'; state.recognition.resetImageRecognition(); const second = state.recognition.runQrScan(); await Promise.resolve();
    expect(mocks.cancel).toHaveBeenCalledWith({ taskId: oldTask });
    old.resolve(qr('old')); await first; expect(state.recognition.qrScanning.value).toBe(true); expect(state.recognition.qrResult.value).toBeNull();
    recent.resolve(qr('fresh')); await second; expect(state.recognition.qrResult.value).toEqual(qr('fresh'));
  });
  it('filters QR progress by task and invalidates the callback after reset', async () => {
    let handler!: (event: { payload: QrScanProgress }) => void;
    mocks.listen.mockImplementation(async (_event, callback) => { handler = callback; return vi.fn(); });
    const pending = deferred<QrScanOutcome>(); mocks.qr.mockReturnValueOnce(pending.promise);
    const state = setup(); const operation = state.recognition.runQrScan(); await Promise.resolve();
    const taskId = mocks.qr.mock.calls[0][0].taskId;
    handler({ payload: { task_id: 'unrelated', attempts: 9, total: 20, found: 3 } }); expect(state.recognition.qrProgress.value).toBeNull();
    handler({ payload: { task_id: taskId, attempts: 2, total: 20, found: 1 } }); expect(state.recognition.qrProgress.value).toEqual({ attempts: 2, total: 20, found: 1 });
    state.recognition.resetImageRecognition(); handler({ payload: { task_id: taskId, attempts: 9, total: 20, found: 3 } });
    expect(state.recognition.qrProgress.value).toBeNull(); pending.resolve(qr('old')); await operation;
  });
  it('drops old OCR after A→B→A while leaving the new operation busy', async () => {
    const old = deferred<OcrOutcome>(), recent = deferred<OcrOutcome>(); mocks.ocr.mockReturnValueOnce(old.promise).mockReturnValueOnce(Promise.resolve(ocr('b'))).mockReturnValueOnce(recent.promise);
    const state = setup(); const first = state.recognition.runOcrScan();
    state.node.value = 'file-b'; state.recognition.resetImageRecognition(); await state.recognition.runOcrScan();
    state.node.value = 'file-a'; state.recognition.resetImageRecognition(); const third = state.recognition.runOcrScan();
    old.resolve(ocr('old')); await first; expect(state.recognition.ocrResult.value).toBeNull(); expect(state.recognition.ocrScanning.value).toBe(true);
    recent.resolve(ocr('fresh')); await third; expect(state.recognition.ocrFullText.value).toBe('fresh');
  });
  it('keeps cancellation pending until the QR command finishes and surfaces a cancel error', async () => {
    const pending = deferred<QrScanOutcome>(); mocks.qr.mockReturnValueOnce(pending.promise);
    const state = setup(); const operation = state.recognition.runQrScan(); await Promise.resolve();
    mocks.cancel.mockRejectedValueOnce(new Error('cancel failed')); await state.recognition.cancelQrScan();
    expect(state.recognition.qrScanning.value).toBe(true); expect(mocks.error).toHaveBeenCalledWith('取消二维码扫描失败', 'Error: cancel failed');
    pending.resolve({ ...qr(''), codes: [], cancelled: true }); await operation; expect(state.recognition.qrScanning.value).toBe(false);
  });
  it('clears copy-feedback timers on disposal', async () => {
    vi.useFakeTimers(); const state = setup(); vi.spyOn(navigator.clipboard, 'writeText').mockResolvedValue(undefined);
    await state.recognition.runOcrScan(); await state.recognition.copyQrText('code', 0); await state.recognition.copyAllOcrText();
    expect(vi.getTimerCount()).toBe(2); state.scope.stop(); expect(vi.getTimerCount()).toBe(0);
  });
});
