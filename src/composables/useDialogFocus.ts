import { nextTick, onBeforeUnmount, watch, type Ref } from 'vue';

/**
 * Focus handling for modal dialogs. When `visible` turns true it remembers the
 * element that had focus and moves focus into `container`; when the dialog
 * closes or unmounts, focus returns to that element so keyboard users land
 * back where they started.
 *
 * Give the container `tabindex="-1"` so it can hold focus, and bind Escape on
 * the container itself (`@keydown.esc.stop`). Element-scoped Escape means a
 * dialog stacked on top (e.g. the confirm dialog) only closes itself.
 */
export function useDialogFocus(visible: () => boolean, container: Ref<HTMLElement | null>) {
  let opener: HTMLElement | null = null;

  function restoreFocus() {
    const target = opener;
    opener = null;
    if (target?.isConnected) target.focus();
  }

  watch(visible, async isVisible => {
    if (!isVisible) {
      restoreFocus();
      return;
    }
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    await nextTick();
    if (visible()) container.value?.focus();
  }, { immediate: true });

  onBeforeUnmount(restoreFocus);
}
