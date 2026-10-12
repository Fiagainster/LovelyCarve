// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref, type EffectScope } from 'vue';
import { useSearchHits } from './useSearchHits';
import type { GlobalSearchHit, GlobalSearchPage } from '../../../lib/ipc/search';
const calls = vi.hoisted(() => ({ page: vi.fn(), error: vi.fn() }));
vi.mock('../../../lib/ipc/search', () => ({ globalSearchHitsPage: calls.page }));
vi.mock('../../../composables/useToast', () => ({ notifyError: calls.error }));
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: Error) => void; const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; }); return { promise, resolve, reject }; }
const hit = (sequence: number): GlobalSearchHit => ({ hit_seq: sequence, node_id: `file:${sequence}`, volume_key: 'part:1', file_path: '/file', file_name: 'file', file_size: 1024, file_offset: sequence, match_len: 1, encoding: 'UTF-8', pattern: 'a', pattern_kind: 'keyword', match_count: 1, context_preview: 'a', context_full: 'a', md5: '', sha256: '', source_kind: 'file' });
const page = (items: GlobalSearchHit[], total = items.length): GlobalSearchPage => ({ search_id: 'search', items, total });
let scope: EffectScope;
const image = ref('a'); const search = ref<string | null>('search'); const running = ref(false);
function setup() { scope = effectScope(); return scope.run(() => useSearchHits({ imageId: () => image.value, searchId: () => search.value, running: () => running.value }))!; }
async function settle() { for (let index = 0; index < 8; index++) await nextTick(); }
beforeEach(() => { vi.resetAllMocks(); image.value = 'a'; search.value = 'search'; running.value = false; calls.page.mockResolvedValue(page([])); });
afterEach(() => scope?.stop());
describe('selected search page ownership', () => {
  it('pages completed results with the applied keyword and distinct mode', async () => {
    const first = Array.from({ length: 200 }, (_, index) => hit(index));
    calls.page.mockResolvedValue(page(first, 300));
    const query = setup(); await settle();
    query.filterKeywordDraft.value = '  name  '; query.distinctMode.value = 'file'; query.applyFilters(); await settle();
    calls.page.mockResolvedValueOnce(page([hit(200)], 300));
    await query.ensureHitsThrough(150);
    expect(calls.page).toHaveBeenLastCalledWith({ imageId: 'a', searchId: 'search', skip: 200, limit: 200, keyword: 'name', distinct: 'file' });
    expect(query.hits.value).toEqual([...first, hit(200)]);
    expect(query.hitsTotal.value).toBe(300);
  });
  it('uses the last sequence as incremental cursor and ignores cursor-relative total', async () => {
    running.value = true; calls.page.mockResolvedValueOnce(page([hit(20)], 400));
    const query = setup(); await settle();
    calls.page.mockResolvedValueOnce(page([hit(25)], 500)); await query.pollIncremental();
    expect(calls.page).toHaveBeenLastCalledWith({ imageId: 'a', searchId: 'search', limit: 500, afterSeq: 20 });
    expect(query.hits.value).toEqual([hit(20), hit(25)]); expect(query.hitsTotal.value).toBe(2);
  });
  it('starts a new running request despite an old in-flight request and keeps its lock', async () => {
    running.value = true; const old = deferred<GlobalSearchPage>(); const fresh = deferred<GlobalSearchPage>();
    calls.page.mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise);
    const query = setup(); image.value = 'b';
    expect(calls.page).toHaveBeenCalledTimes(2);
    old.resolve(page([hit(1)])); await settle(); await query.pollIncremental();
    expect(calls.page).toHaveBeenCalledTimes(2); expect(query.loadingHits.value).toBe(true);
    fresh.resolve(page([hit(2)])); await settle();
    expect(query.hits.value).toEqual([hit(2)]); expect(query.loadingHits.value).toBe(false);
  });
  it('rejects A→B→A results and resets the selected hit and filters', async () => {
    const old = deferred<GlobalSearchPage>(); calls.page.mockReturnValueOnce(old.promise);
    const query = setup(); query.selectedHitSeq.value = 1; query.filterKeywordDraft.value = 'old';
    image.value = 'b'; image.value = 'a'; await settle(); old.resolve(page([hit(1)])); await settle();
    expect(query.hits.value).toEqual([]); expect(query.selectedHit.value).toBeNull(); expect(query.filterKeywordDraft.value).toBe('');
  });
  it('reloads terminal totals after incremental mode and ignores disposed read errors', async () => {
    running.value = true; calls.page.mockResolvedValueOnce(page([hit(1)])); const query = setup(); await settle();
    calls.page.mockResolvedValueOnce(page([hit(1)], 5)); running.value = false; await settle(); expect(query.hitsTotal.value).toBe(5);
    const pending = deferred<GlobalSearchPage>(); calls.page.mockReturnValueOnce(pending.promise);
    const request = query.ensureHitsThrough(0); scope.stop(); pending.reject(new Error('late')); await request;
    expect(calls.error).not.toHaveBeenCalled();
  });
});
