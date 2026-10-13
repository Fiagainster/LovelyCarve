import { onBeforeUnmount, ref, watch } from 'vue';
import { filevaultInspect, type FilevaultInventory } from '../../../lib/ipc/encrypted';
import { taskCancel } from '../../../lib/ipc/taskRuntime';

/** The dialog owns metadata queries, cancellation and publication after region changes. */
export function useFilevaultInventory(enabled: () => boolean, request: () => Parameters<typeof filevaultInspect>[0]) {
  const inventory = ref<FilevaultInventory | null>(null);
  const busy = ref(false);
  const error = ref('');
  let generation = 0;
  let taskId: string | undefined;

  function reset() {
    generation++;
    const previous = taskId;
    taskId = undefined;
    inventory.value = null;
    busy.value = false;
    error.value = '';
    if (previous) void taskCancel({ taskId: previous }).catch(cause => console.warn('取消 FileVault 信息读取失败', cause));
  }
  async function load() {
    reset();
    if (!enabled()) return;
    const captured = generation;
    const currentTaskId = crypto.randomUUID();
    taskId = currentTaskId;
    busy.value = true;
    try {
      const result = await filevaultInspect({ ...request(), taskId: currentTaskId });
      if (captured === generation && enabled()) inventory.value = result;
    } catch (cause) {
      if (captured === generation && enabled()) error.value = String(cause);
    } finally {
      if (captured === generation) { busy.value = false; taskId = undefined; }
    }
  }
  watch(() => [enabled(), request().imageId, request().options.partitionId], () => { void load(); }, { immediate: true });
  onBeforeUnmount(reset);
  return { inventory, busy, error, load, reset };
}
