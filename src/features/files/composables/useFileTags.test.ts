// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileTags, flushFileTagWrites } from './useFileTags';
const ipc = vi.hoisted(() => ({ load: vi.fn(), nodes: vi.fn(), save: vi.fn(), changes: vi.fn() }));
vi.mock('../../../lib/ipc/fs/tags', () => ({ fsTagsLoad: ipc.load, fsTagsLoadNodes: ipc.nodes, fsTagsSave: ipc.save, fsTagsApplyChanges: ipc.changes }));
function deferred<T>() { let resolve!: (value: T) => void; return { promise: new Promise<T>(yes => { resolve = yes; }), resolve: (value: T) => resolve(value) }; }
beforeEach(async () => { await flushFileTagWrites(); vi.clearAllMocks(); ipc.save.mockResolvedValue(true); ipc.changes.mockResolvedValue(true); });
afterEach(() => flushFileTagWrites());
describe('case tag writes', () => {
  it('drains the debounce immediately and waits for already-issued writes', async () => {
    const write = deferred<boolean>(); ipc.changes.mockReturnValueOnce(write.promise);
    useFileTags().setTag('old-case-image', ['file'], '重点');
    let finished = false; const flush = flushFileTagWrites().then(() => { finished = true; });
    await Promise.resolve(); expect(finished).toBe(false);
    expect(ipc.changes).toHaveBeenCalledWith({ imageId: 'old-case-image', upserts: { file: '重点' }, deleteNodeIds: [] });
    write.resolve(true); await flush; expect(localStorage.getItem('lc.fileTags')).toContain('old-case-image');
  });
  it('ignores old hydration that would otherwise queue a write into a new case', async () => {
    const late = deferred<Record<string, string>>(); ipc.nodes.mockReturnValueOnce(late.promise);
    const tags = useFileTags(); const reading = tags.hydrateTagsForNodes('hydration-image', ['file']);
    await flushFileTagWrites(); late.resolve({ file: '重点' }); await reading; await flushFileTagWrites();
    expect(tags.tagOf('hydration-image', 'file')).toBeUndefined(); expect(ipc.changes).not.toHaveBeenCalled();
    ipc.nodes.mockResolvedValueOnce({ file: '可疑' }); await tags.hydrateTagsForNodes('hydration-image', ['file']);
    expect(tags.tagOf('hydration-image', 'file')).toBe('可疑');
  });
});
