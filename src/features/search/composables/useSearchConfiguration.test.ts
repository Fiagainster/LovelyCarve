// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref } from 'vue';
import type { FsNode } from '../../../lib/ipc/fs/types';
import { useSearchConfiguration } from './useSearchConfiguration';

const children = vi.hoisted(() => vi.fn());
vi.mock('../../../lib/ipc/fs/query', () => ({ fsChildren: children }));
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done; }); return { promise, resolve }; }
const partition = (id: string) => ({ id, name: id } as FsNode);
function setup() { const scope = effectScope(); const image = ref('a'); const submitted = vi.fn(); const owner = scope.run(() => useSearchConfiguration({ imageId: () => image.value, onSubmit: submitted }))!; return { scope, image, submitted, owner }; }
async function settle() { for (let index = 0; index < 5; index++) await nextTick(); }
beforeEach(() => { children.mockReset().mockResolvedValue([]); });

describe('search configuration ownership', () => {
  it('filters partition choices and caches them across dialog reopen while preserving the draft', async () => {
    children.mockResolvedValue([partition('part:1'), partition('root')]); const { owner, scope } = setup();
    owner.dialog.keysText = 'keyword'; await owner.openCreateDialog();
    expect(owner.partitionOptions.value).toEqual([{ id: 'part:1', label: 'part:1', title: 'part:1（part:1）' }]);
    owner.closeCreateDialog(); await owner.openCreateDialog(); expect(children).toHaveBeenCalledOnce(); expect(owner.dialog.keysText).toBe('keyword'); scope.stop();
  });
  it('ignores old choices after A→B→A and keeps the newest request busy', async () => {
    const old = deferred<FsNode[]>(); const newest = deferred<FsNode[]>(); children.mockReturnValueOnce(old.promise).mockResolvedValueOnce([]).mockReturnValueOnce(newest.promise);
    const { owner, image, scope } = setup(); const opening = owner.openCreateDialog(); image.value = 'b'; image.value = 'a';
    old.resolve([partition('part:old')]); await opening; expect(owner.partitionOptions.value).toEqual([]); expect(owner.loadingPartitions.value).toBe(true);
    newest.resolve([partition('part:new')]); await settle(); expect(owner.partitionOptions.value[0]?.id).toBe('part:new'); expect(owner.loadingPartitions.value).toBe(false); scope.stop();
  });
  it('does not focus or publish choices after close/disposal', async () => {
    const listed = deferred<FsNode[]>(); children.mockReturnValue(listed.promise); const { owner, scope } = setup();
    const field = document.createElement('textarea'); field.focus = vi.fn(); owner.elements.keyword.value = field;
    const opening = owner.openCreateDialog(); owner.closeCreateDialog(); scope.stop(); listed.resolve([partition('part:old')]); await opening; await settle();
    expect(field.focus).not.toHaveBeenCalled(); expect(owner.partitionOptions.value).toEqual([]);
  });
  it('validates before serializing and closes the dialog before dispatching a search', async () => {
    const { owner, submitted, scope } = setup(); await owner.openCreateDialog(); owner.submitCreateDialog(); expect(submitted).not.toHaveBeenCalled();
    owner.dialog.keysText = 'key'; owner.dialog.scopeMode = 'tree_nodes'; owner.dialog.treeNodesText = 'Users/alice\n/Windows';
    submitted.mockImplementation(() => { expect(owner.dialogVisible.value).toBe(false); }); owner.submitCreateDialog();
    expect(submitted.mock.calls[0]?.[0].scope).toEqual({ partitions: [], tags: [], tree_nodes: ['/Users/alice', '/Windows'] }); scope.stop();
  });
});
