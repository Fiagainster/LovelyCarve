// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref, type EffectScope } from 'vue';
import { useSearchHistory } from './useSearchHistory';
import type { GlobalSearchHistoryEntry } from '../../../lib/ipc/search';
const calls = vi.hoisted(() => ({ history: vi.fn(), remove: vi.fn(), cancel: vi.fn(), confirm: vi.fn(), error: vi.fn(), info: vi.fn() }));
vi.mock('../../../lib/ipc/search', () => ({ globalSearchHistoryList: calls.history, globalSearchHistoryDelete: calls.remove }));
vi.mock('../../../lib/ipc/taskRuntime', () => ({ taskCancel: calls.cancel }));
vi.mock('../../../composables/useConfirm', () => ({ confirm: calls.confirm }));
vi.mock('../../../composables/useToast', () => ({ notifyError: calls.error, notifyInfo: calls.info }));
const task = (id = 'search'): GlobalSearchHistoryEntry => ({ search_id: id, image_id: 'a', keywords: ['key'], regexes: [], codepages: ['UTF-8'], match_case: false, search_deleted: false, search_empty_space: false, calc_hash: false, scope_json: '{}', status: 'running', error: null, started_ms: 1, finished_ms: null, files_total: 10, files_scanned: 0, matched_file_count: 0, total_matches: 0, record_count: 0, read_errors: 0, truncated: false });
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
let scope: EffectScope; const image = ref('a');
function setup() { scope = effectScope(); return scope.run(() => useSearchHistory({ imageId: () => image.value }))!; }
async function settle() { for (let index = 0; index < 8; index++) await nextTick(); }
beforeEach(() => { vi.resetAllMocks(); image.value = 'a'; calls.history.mockResolvedValue([task()]); calls.confirm.mockResolvedValue(true); });
afterEach(() => scope?.stop());
describe('search history context', () => {
  it('ignores an older overlapping list and a missing explicit selection', async () => {
    const old = deferred<GlobalSearchHistoryEntry[]>(); calls.history.mockReturnValueOnce(old.promise);
    const history = setup(); await history.refreshTasks('missing'); old.resolve([task('old')]); await settle();
    expect(history.tasks.value).toEqual([task()]); expect(history.selectedSearchId.value).toBe('search');
  });
  it('ignores the original A→B→A history and does not clear the current busy state', async () => {
    const old = deferred<GlobalSearchHistoryEntry[]>(); const fresh = deferred<GlobalSearchHistoryEntry[]>();
    calls.history.mockReturnValueOnce(old.promise).mockResolvedValueOnce([]).mockReturnValueOnce(fresh.promise);
    const history = setup(); image.value = 'b'; image.value = 'a'; old.resolve([task('old')]); await settle();
    expect(history.loadingTasks.value).toBe(true); expect(history.tasks.value).toEqual([]);
    fresh.resolve([task()]); await settle(); expect(history.tasks.value).toEqual([task()]);
  });
  it('does not delete after a confirmation belonging to an old evidence view', async () => {
    const choice = deferred<boolean>(); calls.confirm.mockReturnValue(choice.promise); const history = setup(); await settle();
    const deletion = history.deleteTask(task()); image.value = 'b'; choice.resolve(true); await deletion;
    expect(calls.remove).not.toHaveBeenCalled();
  });
  it('captures the evidence identity for deletion and refreshes the current list', async () => {
    calls.remove.mockResolvedValue(true); const history = setup(); await settle(); await history.deleteTask(task());
    expect(calls.remove).toHaveBeenCalledExactlyOnceWith({ imageId: 'a', searchId: 'search' });
    expect(history.deletingTaskId.value).toBeNull();
  });
  it('does not resurrect a terminal history item from late progress', async () => {
    calls.history.mockResolvedValue([{ ...task(), status: 'success' }]); const history = setup(); await settle();
    expect(history.acceptProgress({ image_id: 'a', search_id: 'search', files_done: 1, files_total: 10, hits: 1, matched_files: 1 })).toBe(false);
    expect(history.selectedTask.value?.status).toBe('success'); expect(history.runningProgress.size).toBe(0);
  });
  it('keeps cancellation ownership on the selected task', async () => {
    const first = deferred<void>(); const second = deferred<void>(); calls.cancel.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const history = setup(); await settle(); const old = history.cancelSelectedTask(); history.selectTask(task('next')); const fresh = history.cancelSelectedTask();
    first.resolve(); await old; expect(history.cancelling.value).toBe(true); expect(calls.info).not.toHaveBeenCalled();
    second.resolve(); await fresh; expect(history.cancelling.value).toBe(false); expect(calls.cancel.mock.calls).toEqual([[{ taskId: 'search' }], [{ taskId: 'next' }]]);
  });
});
