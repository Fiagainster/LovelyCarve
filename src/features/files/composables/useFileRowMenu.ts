import { computed, nextTick, onActivated, onDeactivated, onMounted, onScopeDispose, reactive, ref, type Ref } from 'vue';
import { isHostDockerMountRoot, rowKeyOf, type DiskFsNode } from '../model/model';

/** Owns the floating row menu and visible-panel Escape precedence. */
export function useFileRowMenu(options: {
  selectedKey: Ref<string | null>;
  previewVisible: Ref<boolean>;
  filterVisible: () => boolean;
  closeFilter: () => void;
  exportModeOpen: () => boolean;
  cancelExportMode: () => void;
  hasChecked: () => boolean;
  clearChecked: () => void;
  refreshMounts: () => Promise<void>;
}) {
  const rowMenu = reactive({ visible: false, x: 0, y: 0, node: null as DiskFsNode | null });
  const hashMenuOpen = ref(false);
  const hashMenuFlip = computed(() => rowMenu.x + 200 + 120 > window.innerWidth);
  let listenerGeneration = 0;
  let active = true;
  let disposed = false;

  function hideRowMenu() {
    listenerGeneration++;
    rowMenu.visible = false;
    hashMenuOpen.value = false;
    document.removeEventListener('click', hideRowMenu);
    document.removeEventListener('contextmenu', hideRowMenu);
  }

  function onRowContextMenu(event: MouseEvent, node: DiskFsNode) {
    if (!active || disposed) return;
    hideRowMenu();
    options.closeFilter();
    rowMenu.node = node;
    rowMenu.x = Math.min(event.clientX, window.innerWidth - 200 - 8);
    rowMenu.y = Math.min(event.clientY, window.innerHeight - (isHostDockerMountRoot(node) ? 436 : 402) - 8);
    rowMenu.visible = true;
    options.selectedKey.value = rowKeyOf(node);
    const generation = listenerGeneration;
    void nextTick(() => {
      if (disposed || !active || !rowMenu.visible || generation !== listenerGeneration) return;
      document.addEventListener('click', hideRowMenu, { once: true });
      document.addEventListener('contextmenu', hideRowMenu, { once: true });
    });
    if (isHostDockerMountRoot(node)) void options.refreshMounts();
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.isComposing || event.keyCode === 229) return;
    if (event.key === 'Escape') {
      if (rowMenu.visible) hideRowMenu();
      else if (options.filterVisible()) options.closeFilter();
      else if (options.exportModeOpen()) options.cancelExportMode();
      else {
        const target = event.target as HTMLElement | null;
        if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return;
        if (options.previewVisible.value) options.previewVisible.value = false;
        else if (options.hasChecked()) options.clearChecked();
      }
      return;
    }
    if (!rowMenu.visible || (event.key !== 'ArrowDown' && event.key !== 'ArrowUp')) return;
    event.preventDefault();
    const items = Array.from(document.querySelectorAll<HTMLButtonElement>('.disk-row-menu .disk-row-menu-item:not(:disabled)'));
    if (!items.length) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const step = event.key === 'ArrowDown' ? 1 : -1;
    const next = current < 0 ? (step === 1 ? 0 : items.length - 1) : (current + step + items.length) % items.length;
    items[next].focus();
  }

  function activate() {
    active = true;
    document.addEventListener('keydown', onKeydown);
  }
  function deactivate() {
    active = false;
    document.removeEventListener('keydown', onKeydown);
    hideRowMenu();
  }
  onMounted(activate);
  onActivated(activate);
  onDeactivated(deactivate);
  onScopeDispose(() => { disposed = true; deactivate(); });
  return { rowMenu, hashMenuOpen, hashMenuFlip, hideRowMenu, onRowContextMenu };
}
