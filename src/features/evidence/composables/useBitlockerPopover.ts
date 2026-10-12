import { nextTick, onActivated, onDeactivated, onMounted, onScopeDispose, ref, watch } from 'vue';
import type { BitlockerInputMode } from '../model/bitlocker';

interface BitlockerPopoverOptions {
  context: () => string;
  mode: () => BitlockerInputMode;
  content: () => readonly unknown[];
  dismissReminder: () => void;
  focusInput: () => void;
  onClose: () => void;
}

/** Owns only the anchored UI; closing it leaves backend unlock/scan jobs running. */
export function useBitlockerPopover(options: BitlockerPopoverOptions) {
  const controlEl = ref<HTMLElement | null>(null);
  const popoverEl = ref<HTMLElement | null>(null);
  const popoverOpen = ref(false);
  let generation = 0;
  let disposed = false;
  let repositionRaf = 0;
  let listening = false;
  function current(captured: number) { return !disposed && popoverOpen.value && captured === generation; }
  async function openPopover() {
    if (disposed) return;
    const captured = ++generation;
    popoverOpen.value = true;
    options.dismissReminder();
    await nextTick();
    if (current(captured)) positionPopover();
  }
  async function togglePopover() {
    if (popoverOpen.value) { closePopover(); return; }
    const captured = generation + 1;
    await openPopover();
    if (current(captured)) options.focusInput();
  }
  function closePopover() {
    generation += 1;
    popoverOpen.value = false;
    options.onClose();
  }
  /** Position the body-teleported popover with `position: fixed`, anchored to
   *  the control's right edge (matching the old `right: 0` design), then clamp
   *  inside the viewport so neither edge is clipped. */
  function positionPopover() {
    const anchor = controlEl.value;
    const pop = popoverEl.value;
    if (!anchor || !pop) return;
    const a = anchor.getBoundingClientRect();
    const margin = 8;
    const width = pop.offsetWidth;
    // Right-align to the control, then clamp to [margin, viewport - margin].
    let left = a.right - width;
    left = Math.min(left, window.innerWidth - width - margin);
    left = Math.max(margin, left);
    // Prefer below the control; flip above if it doesn't fit below but does
    // above; if it fits neither (taller than the viewport allows either way),
    // pin to the top margin so the popover's own `overflow-y: auto` scrolls and
    // the bottom action buttons stay reachable.
    let top = a.bottom + 6;
    const height = pop.offsetHeight;
    const overflowsBelow = top + height > window.innerHeight - margin;
    const roomAbove = a.top - 6 - height > margin;
    if (overflowsBelow && roomAbove) {
      top = a.top - 6 - height;
    } else if (overflowsBelow) {
      top = margin;
    }
    top = Math.max(margin, top);
    pop.style.setProperty('--bde-pop-top', `${top}px`);
    pop.style.setProperty('--bde-pop-left', `${left}px`);
  }


  function onReposition() {
    if (!popoverOpen.value || repositionRaf) return;
    const captured = generation;
    repositionRaf = requestAnimationFrame(() => {
      repositionRaf = 0;
      if (current(captured)) positionPopover();
    });
  }
  function removePositionListeners() {
    window.removeEventListener('scroll', onReposition, true);
    window.removeEventListener('resize', onReposition);
    if (repositionRaf) cancelAnimationFrame(repositionRaf);
    repositionRaf = 0;
  }
  function onEsc(event: KeyboardEvent) {
    if (event.key === 'Escape' && !event.isComposing && popoverOpen.value) closePopover();
  }
  function onDocClick(event: MouseEvent) {
    if (event.target instanceof Element && event.target.closest('.ui-select-list')) return;
    if (event.target instanceof Node && (controlEl.value?.contains(event.target) || popoverEl.value?.contains(event.target))) return;
    if (popoverOpen.value) closePopover();
  }
  function attach() {
    if (disposed || listening) return;
    listening = true;
    document.addEventListener('click', onDocClick);
    document.addEventListener('keydown', onEsc);
  }
  function detach() {
    listening = false;
    closePopover();
    document.removeEventListener('click', onDocClick);
    document.removeEventListener('keydown', onEsc);
    removePositionListeners();
  }
  watch(options.context, closePopover, { flush: 'sync' });
  watch(options.mode, async () => {
    const captured = generation;
    await nextTick();
    if (!current(captured)) return;
    options.focusInput(); positionPopover();
  });
  watch(options.content, async () => {
    const captured = generation;
    await nextTick();
    if (current(captured)) positionPopover();
  });
  watch(popoverOpen, open => {
    if (open) {
      window.addEventListener('scroll', onReposition, true);
      window.addEventListener('resize', onReposition);
    } else removePositionListeners();
  }, { flush: 'sync' });
  onMounted(attach);
  onActivated(attach);
  onDeactivated(detach);
  onScopeDispose(() => { disposed = true; detach(); });
  return { controlEl, popoverEl, popoverOpen, openPopover, closePopover, togglePopover };
}
