// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { effectScope, ref, type EffectScope } from 'vue';
import { useTimelineRows } from './useTimelineRows';
import type { TimelineDataPage, TimelineQuery, TimelineRow } from '../../../lib/ipc/timeline';
const calls = vi.hoisted(() => ({ datas: vi.fn(), error: vi.fn() }));
vi.mock('../../../lib/ipc/timeline', () => ({ timelineDatas: calls.datas }));
vi.mock('../../../composables/useToast', () => ({ notifyError: calls.error }));
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: Error) => void; const promise = new Promise<T>((done, fail) => { resolve = done; reject = fail; }); return { promise, resolve, reject }; }
const row = (id: number): TimelineRow => ({ id, oid: `file:${id}`, eid: 'a', tid: '', deleted: false, tl_time: 1, from_app: 'files', data_type: 1, tl_flag: '修改', content: 'file', extra: '' });
const filter: TimelineQuery = { time_zone_hours: 8, time_unit: 'year', from_apps: [], data_types: [], pairs: [], show_delete: false };
let scope: EffectScope; const image = ref('a');
function setup() { scope = effectScope(); return scope.run(() => useTimelineRows({ imageId: () => image.value, buildQuery: () => filter }))!; }
beforeEach(() => { vi.resetAllMocks(); image.value = 'a'; calls.datas.mockResolvedValue({ items: [row(1)], total: 3 }); });
afterEach(() => scope?.stop());
it('keeps completed pagination, query parameters and order together', async () => {
  const query = setup(); await query.loadRowsFirstPage(); calls.datas.mockResolvedValueOnce({ items: [row(2)], total: 3 });
  await query.ensureRowsThrough(61);
  expect(calls.datas).toHaveBeenLastCalledWith({ imageId: 'a', query: filter, skip: 1, limit: 200, order: 'desc' });
  expect(query.rows.value).toEqual([row(1), row(2)]);
});
it('keeps the new page single-flight when an older page completes', async () => {
  const old = deferred<TimelineDataPage>(); const fresh = deferred<TimelineDataPage>();
  const query = setup(); await query.loadRowsFirstPage(); calls.datas.mockReturnValueOnce(old.promise);
  const oldPage = query.ensureRowsThrough(61); image.value = 'b'; query.reset(); await query.loadRowsFirstPage();
  calls.datas.mockReturnValueOnce(fresh.promise); const newPage = query.ensureRowsThrough(61);
  old.resolve({ items: [row(99)], total: 3 }); await oldPage;
  const count = calls.datas.mock.calls.length; await query.ensureRowsThrough(61); expect(calls.datas).toHaveBeenCalledTimes(count);
  fresh.resolve({ items: [row(2)], total: 3 }); await newPage; expect(query.rows.value).toEqual([row(1), row(2)]);
});
it('rejects an old first page after reset and does not publish a disposed error', async () => {
  const old = deferred<TimelineDataPage>(); calls.datas.mockReturnValueOnce(old.promise);
  const query = setup(); const first = query.loadRowsFirstPage(); query.reset(); await query.loadRowsFirstPage();
  old.resolve({ items: [row(99)], total: 99 }); await first; expect(query.rows.value).toEqual([row(1)]);
  const pending = deferred<TimelineDataPage>(); calls.datas.mockReturnValueOnce(pending.promise); const page = query.ensureRowsThrough(61);
  scope.stop(); pending.reject(new Error('late')); await page; expect(calls.error).not.toHaveBeenCalled();
});
