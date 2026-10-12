// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, defineComponent, h, KeepAlive, nextTick, ref, shallowRef, type App } from 'vue';
import { useFileTableViewport } from './useFileTableViewport';
import type { DiskFsNode } from '../model/model';

let app: App | undefined;
let host: HTMLDivElement;
let nextFrame = 0;
const frames = new Map<number, FrameRequestCallback>();
beforeEach(() => {
  frames.clear(); nextFrame = 0;
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => { frames.set(++nextFrame, callback); return nextFrame; });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => { frames.delete(id); });
});
afterEach(() => { app?.unmount(); app = undefined; host?.remove(); vi.unstubAllGlobals(); });
function frame() { const [id, callback] = [...frames][0]; frames.delete(id); callback(0); }
async function setup() {
  const entries = shallowRef(Array<DiskFsNode>(100_001).fill({ id: 'file', name: 'file' } as DiskFsNode));
  const visible = ref(true), sortKey = ref<string | null>(null), sortDir = ref<'asc' | 'desc'>('asc');
  let generation = 0;
  let viewport!: ReturnType<typeof useFileTableViewport>;
  const Panel = defineComponent({ setup() {
    viewport = useFileTableViewport({ entries, rows: entries, loading: ref(false), flatMode: ref(false),
      container: ref(null), rowHeight: 36, virtualCount: ref(0), selectedKey: ref(null), detailNode: ref(null), pendingRestore: ref(null),
      sortKey, sortDir, flatSignature: ref('flat'), directorySignature: ref('directory'), generation: () => generation,
      flatQuery: { flatTotal: ref(0), ensureFlatLoadedThrough: async () => {}, loadedSignature: () => 'flat' },
      directoryQuery: { loadDirectoryNextPage: async () => {}, loadedSignature: () => 'directory' },
      loadCurrent: async () => {}, hydrateVisible: () => {}, scrollToRow: () => {} });
    return () => h('div');
  } });
  host = document.createElement('div'); document.body.append(host);
  app = createApp({ render: () => h(KeepAlive, null, { default: () => visible.value ? h(Panel) : null }) });
  app.mount(host); await nextTick();
  return { viewport, visible, sortKey, sortDir, invalidate: () => { generation++; } };
}

describe('deferred large-file-table sort', () => {
  it('shows busy feedback before sorting and applies one sort after two frames', async () => {
    const state = await setup(); state.viewport.cycleSort('name'); state.viewport.cycleSort('size');
    expect(state.sortKey.value).toBeNull(); expect(state.viewport.sortBusyKey.value).toBe('name');
    frame(); expect(state.sortKey.value).toBeNull(); frame(); await nextTick();
    expect(state.sortKey.value).toBe('name'); expect(state.viewport.sortBusyKey.value).toBeNull();
  });
  it('does not sort new evidence from an old queued click', async () => {
    const state = await setup(); state.viewport.cycleSort('name'); frame(); state.invalidate(); frame(); await nextTick();
    expect(state.sortKey.value).toBeNull(); expect(state.viewport.sortBusyKey.value).toBeNull();
  });
  it('cancels queued frames when cached or destroyed and stops accepting disposed actions', async () => {
    const state = await setup(); state.viewport.cycleSort('name'); state.visible.value = false; await nextTick();
    expect(frames.size).toBe(0); expect(state.sortKey.value).toBeNull();
    state.visible.value = true; await nextTick(); state.viewport.cycleSort('size'); frame();
    app!.unmount(); app = undefined; expect(frames.size).toBe(0);
    state.viewport.cycleSort('name'); expect(frames.size).toBe(0); expect(state.sortKey.value).toBeNull();
  });
});
