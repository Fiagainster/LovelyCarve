// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref, type EffectScope } from 'vue';
import { useZipArchive } from './useZipArchive';
import type { ZipPreview } from '../../../lib/ipc/zip';
const read = vi.hoisted(() => vi.fn());
vi.mock('../../../lib/ipc/zip', () => ({ fsZipPreview: read }));
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
let scope: EffectScope; const image = ref('a'); const node = ref('zip');
async function settle() { for (let index = 0; index < 8; index++) await nextTick(); }
function setup() { scope = effectScope(); return scope.run(() => useZipArchive({ imageId: () => image.value, nodeId: () => node.value }))!; }
beforeEach(() => { vi.resetAllMocks(); image.value = 'a'; node.value = 'zip'; });
afterEach(() => scope?.stop());
it('keeps the fresh A→B→A archive busy when an old response returns', async () => {
  const old = deferred<ZipPreview>(); const fresh = deferred<ZipPreview>(); read.mockReturnValueOnce(old.promise).mockResolvedValueOnce({ entries: [] }).mockReturnValueOnce(fresh.promise);
  const archive = setup(); image.value = 'b'; image.value = 'a'; old.resolve({ entries: [], comment: 'old' } as unknown as ZipPreview); await settle();
  expect(archive.report.value).toBeNull(); expect(archive.loading.value).toBe(true);
  fresh.resolve({ entries: [], comment: 'fresh' } as unknown as ZipPreview); await settle(); expect(archive.report.value?.comment).toBe('fresh');
});
