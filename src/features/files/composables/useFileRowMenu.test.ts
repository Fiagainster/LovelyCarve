// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createApp, defineComponent, h, KeepAlive, nextTick, ref, type App } from 'vue';
import { useFileRowMenu } from './useFileRowMenu';
import type { DiskFsNode } from '../model/model';

let app: App | undefined;
let host: HTMLElement;
afterEach(() => { app?.unmount(); app = undefined; host?.remove(); vi.restoreAllMocks(); });
async function setup() {
  const visible = ref(true), preview = ref(true), selectedKey = ref<string | null>(null);
  const filter = ref(false), exportMode = ref(false), checked = ref(true);
  const closeFilter = vi.fn(() => { filter.value = false; });
  const cancelExportMode = vi.fn(() => { exportMode.value = false; });
  const clearChecked = vi.fn(() => { checked.value = false; });
  let menu!: ReturnType<typeof useFileRowMenu>;
  const Panel = defineComponent({ setup() {
    menu = useFileRowMenu({ selectedKey, previewVisible: preview, filterVisible: () => filter.value,
      closeFilter, exportModeOpen: () => exportMode.value, cancelExportMode,
      hasChecked: () => checked.value, clearChecked, refreshMounts: async () => {} });
    return () => h('div');
  } });
  host = document.createElement('div'); document.body.append(host);
  app = createApp({ render: () => h(KeepAlive, null, { default: () => visible.value ? h(Panel) : null }) });
  app.mount(host); await nextTick();
  function key(key: string, target: HTMLElement = document.body, isComposing = false) {
    target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, isComposing }));
  }
  const node = { id: 'file', rowKey: 'path:file', kind: 'file' } as DiskFsNode;
  return { menu, visible, preview, selectedKey, filter, exportMode, checked, closeFilter, cancelExportMode, clearChecked, key, node };
}

describe('floating file menu lifecycle', () => {
  it('closes exactly one Escape layer and respects IME/input ownership', async () => {
    const state = await setup();
    state.menu.onRowContextMenu(new MouseEvent('contextmenu'), state.node);
    state.filter.value = true; state.exportMode.value = true;
    state.key('Escape', document.body, true);
    expect(state.menu.rowMenu.visible).toBe(true);
    state.key('Escape');
    expect(state.menu.rowMenu.visible).toBe(false);
    expect(state.filter.value).toBe(true);
    state.key('Escape'); expect(state.filter.value).toBe(false);
    state.key('Escape'); expect(state.exportMode.value).toBe(false);
    const input = document.createElement('input'); host.append(input);
    state.key('Escape', input); expect(state.preview.value).toBe(true);
    state.key('Escape'); expect(state.preview.value).toBe(false);
    expect(state.checked.value).toBe(true);
    state.key('Escape'); expect(state.checked.value).toBe(false);
  });
  it('does not attach a deferred outside listener after the menu has closed', async () => {
    const state = await setup();
    const add = vi.spyOn(document, 'addEventListener');
    state.menu.onRowContextMenu(new MouseEvent('contextmenu'), state.node);
    state.menu.hideRowMenu();
    await nextTick();
    expect(add.mock.calls.filter(([name]) => name === 'click' || name === 'contextmenu')).toEqual([]);
  });
  it('removes keyboard ownership when cached and restores it on activation', async () => {
    const state = await setup();
    state.menu.onRowContextMenu(new MouseEvent('contextmenu'), state.node);
    state.visible.value = false; await nextTick();
    expect(state.menu.rowMenu.visible).toBe(false);
    state.key('Escape'); expect(state.preview.value).toBe(true);
    state.menu.onRowContextMenu(new MouseEvent('contextmenu'), state.node);
    expect(state.menu.rowMenu.visible).toBe(false);
    state.visible.value = true; await nextTick();
    state.key('Escape'); expect(state.preview.value).toBe(false);
    app!.unmount(); app = undefined;
    state.preview.value = true;
    state.key('Escape'); expect(state.preview.value).toBe(true);
  });
  it('focuses only enabled menu actions while preserving the selected hard-link row', async () => {
    const state = await setup();
    state.menu.onRowContextMenu(new MouseEvent('contextmenu'), state.node);
    expect(state.selectedKey.value).toBe('path:file');
    const list = document.createElement('div'); list.className = 'disk-row-menu'; host.append(list);
    const items = [false, true, false].map(disabled => {
      const button = document.createElement('button'); button.className = 'disk-row-menu-item'; button.disabled = disabled;
      list.append(button); return button;
    });
    state.key('ArrowDown'); expect(document.activeElement).toBe(items[0]);
    state.key('ArrowDown'); expect(document.activeElement).toBe(items[2]);
    state.key('ArrowDown'); expect(document.activeElement).toBe(items[0]);
    state.key('ArrowUp'); expect(document.activeElement).toBe(items[2]);
  });
});
