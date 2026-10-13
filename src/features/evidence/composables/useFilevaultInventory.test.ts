// @vitest-environment happy-dom
import { createApp, defineComponent, h, nextTick, ref, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { FilevaultInventory } from '../../../lib/ipc/encrypted';
import { useFilevaultInventory } from './useFilevaultInventory';
const rpc = vi.hoisted(() => ({ inspect: vi.fn(), cancel: vi.fn() }));
vi.mock('../../../lib/ipc/encrypted', () => ({ filevaultInspect: rpc.inspect }));
vi.mock('../../../lib/ipc/taskRuntime', () => ({ taskCancel: rpc.cancel }));
const enabled = ref(false);
const imageId = ref('a');
let app: App | undefined;
let controller: ReturnType<typeof useFilevaultInventory>;
function deferred() {
  let resolve!: (result: FilevaultInventory) => void;
  const promise = new Promise<FilevaultInventory>(done => { resolve = done; });
  return { promise, resolve };
}
const inventory = (name: string) => ({ volumes: [{ index: 0, name }] } as FilevaultInventory);
beforeEach(() => {
  vi.clearAllMocks(); enabled.value = false; imageId.value = 'a';
  rpc.cancel.mockResolvedValue(undefined);
  app = createApp(defineComponent({ setup() {
    controller = useFilevaultInventory(() => enabled.value, () => ({ imageId: imageId.value, options: {} }));
    return () => h('div');
  } }));
  app.mount(document.createElement('div'));
});
afterEach(() => app?.unmount());
describe('FileVault metadata query ownership', () => {
  it('cancels the old region and ignores its late result', async () => {
    const first = deferred(); const second = deferred();
    rpc.inspect.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    enabled.value = true; await nextTick();
    const oldId = rpc.inspect.mock.calls[0][0].taskId;
    imageId.value = 'b'; await nextTick();
    expect(rpc.cancel).toHaveBeenCalledWith({ taskId: oldId });
    second.resolve(inventory('new')); await nextTick();
    first.resolve(inventory('stale')); await nextTick();
    expect(controller.inventory.value?.volumes[0].name).toBe('new');
    expect(controller.busy.value).toBe(false);
  });
  it('closing the dialog revokes publication and cancels the query', async () => {
    const pending = deferred(); rpc.inspect.mockReturnValue(pending.promise);
    enabled.value = true; await nextTick();
    enabled.value = false; await nextTick();
    pending.resolve(inventory('closed')); await nextTick();
    expect(rpc.cancel).toHaveBeenCalledOnce();
    expect(controller.inventory.value).toBeNull();
    expect(controller.busy.value).toBe(false);
  });
  it('unmount cancels a running query without publishing its result', async () => {
    const pending = deferred(); rpc.inspect.mockReturnValue(pending.promise);
    enabled.value = true; await nextTick(); app?.unmount(); app = undefined;
    pending.resolve(inventory('late')); await nextTick();
    expect(rpc.cancel).toHaveBeenCalledOnce();
    expect(controller.inventory.value).toBeNull();
  });
});
