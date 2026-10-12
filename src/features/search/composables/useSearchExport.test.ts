// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { effectScope, ref, type EffectScope } from 'vue';
import { useSearchExport } from './useSearchExport';
import type { GlobalSearchHistoryEntry } from '../../../lib/ipc/search';
const calls = vi.hoisted(() => ({ save: vi.fn(), export: vi.fn(), success: vi.fn(), error: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ save: calls.save }));
vi.mock('../../../lib/ipc/search', () => ({ globalSearchExport: calls.export }));
vi.mock('../../../composables/useToast', () => ({ notifyExportSuccess: calls.success, notifyError: calls.error }));
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
let scope: EffectScope; const image = ref('a'); const keyword = ref('first');
const task = ref({ search_id: 'search', keywords: ['key'], regexes: [] } as GlobalSearchHistoryEntry);
function setup() { scope = effectScope(); return scope.run(() => useSearchExport({ imageId: () => image.value, selectedTask: () => task.value, keyword: () => keyword.value, distinct: () => 'file' }))!; }
beforeEach(() => { vi.resetAllMocks(); image.value = 'a'; keyword.value = 'first'; calls.export.mockResolvedValue({ output_path: 'D:/result.csv', record_count: 5 }); });
afterEach(() => scope?.stop());
it('captures filters before the chooser and prevents repeated chooser clicks', async () => {
  const destination = deferred<string>(); calls.save.mockReturnValueOnce(destination.promise);
  const action = setup(); const exportResult = action.exportResults(); keyword.value = 'new'; await action.exportResults();
  expect(calls.save).toHaveBeenCalledOnce(); destination.resolve('D:/result.csv'); await exportResult;
  expect(calls.export).toHaveBeenCalledExactlyOnceWith({ imageId: 'a', searchId: 'search', destination: 'D:/result.csv', keyword: 'first', distinct: 'file' });
  expect(action.exporting.value).toBe(false); expect(calls.success).toHaveBeenCalledWith('D:/result.csv', '已导出 5 条命中');
});
it('does not start an old export after the evidence chooser context changes', async () => {
  const destination = deferred<string>(); calls.save.mockReturnValueOnce(destination.promise);
  const action = setup(); const exportResult = action.exportResults(); image.value = 'b'; image.value = 'a'; destination.resolve('D:/old.csv'); await exportResult;
  expect(calls.export).not.toHaveBeenCalled(); expect(action.exporting.value).toBe(false);
});
it('keeps a new chooser busy when an older one completes', async () => {
  const old = deferred<string>(); const fresh = deferred<string>(); calls.save.mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise);
  const action = setup(); const previous = action.exportResults(); image.value = 'b'; const current = action.exportResults();
  old.resolve('D:/old.csv'); await previous; expect(action.exporting.value).toBe(true);
  fresh.resolve('D:/new.csv'); await current; expect(calls.export.mock.calls[0][0].imageId).toBe('b'); expect(action.exporting.value).toBe(false);
});
