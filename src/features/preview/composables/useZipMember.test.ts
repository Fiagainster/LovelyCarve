// @vitest-environment happy-dom
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref, type EffectScope } from 'vue';
import { useZipMember } from './useZipMember';
import type { ZipPreviewEntry } from '../../../lib/ipc/zip';
const read = vi.hoisted(() => vi.fn());
vi.mock('../../../lib/ipc/zip', () => ({ fsZipMemberRead: read }));
const entry = (path = 'file'): ZipPreviewEntry => ({ path, name: path, kind: 'file', size: 10, compressed_size: 10, method: 'Stored', encrypted: false, crc32: null, modified: null, link_target: null });
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
let scope: EffectScope; const image = ref('a'); const node = ref('zip');
function setup() { scope = effectScope(); return scope.run(() => useZipMember({ imageId: () => image.value, nodeId: () => node.value }))!; }
beforeEach(() => { vi.resetAllMocks(); image.value = 'a'; node.value = 'zip'; });
afterEach(() => scope?.stop());
it('rejects a stale member read after switching to another member and back', async () => {
  const old = deferred<ArrayBuffer>(); read.mockReturnValueOnce(old.promise).mockResolvedValueOnce(new Uint8Array([0]).buffer).mockResolvedValueOnce(new TextEncoder().encode('fresh').buffer);
  const member = setup(); const pending = member.selectMember(entry()); await member.selectMember(entry('other')); await member.selectMember(entry());
  old.resolve(new TextEncoder().encode('old').buffer); await pending;
  expect(member.textChunks.value).toEqual(['fresh']); expect(member.memberTab.value).toBe('text');
  expect(read.mock.calls[0][0]).toEqual({ imageId: 'a', nodeId: 'zip', memberPath: 'file', maxBytes: 1024 * 1024 });
});
it('drops reads from an old archive and leaves an unsupported member idle', async () => {
  const old = deferred<ArrayBuffer>(); read.mockReturnValueOnce(old.promise); const member = setup(); const pending = member.selectMember(entry());
  node.value = 'other-zip'; await member.selectMember({ ...entry('link'), kind: 'symlink', link_target: 'target' });
  old.resolve(new TextEncoder().encode('old').buffer); await pending;
  expect(member.memberBytes.value).toBeNull(); expect(member.memberLoading.value).toBe(false); expect(member.selected.value?.path).toBe('link');
});
it('does not publish a disposed read and retains byte truncation and hex limits', async () => {
  const bytes = new Uint8Array(40 * 1024); read.mockResolvedValueOnce(bytes.buffer); const member = setup(); await member.selectMember({ ...entry(), size: 50 * 1024 });
  expect(member.memberTruncated.value).toBe(true); expect(member.hexCapped.value).toBe(true); expect(member.hexRows.value).toHaveLength(32 * 1024 / 16);
  const late = deferred<ArrayBuffer>(); read.mockReturnValueOnce(late.promise); const pending = member.selectMember(entry('late')); scope.stop(); late.resolve(new ArrayBuffer(1)); await pending;
  expect(member.memberBytes.value).toBeNull();
});
