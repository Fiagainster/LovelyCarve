import { onScopeDispose, ref } from 'vue';

interface UseResizablePanelOptions {
  storageKey: string;
  defaultHeight: number;
  minHeight: number;
  maxViewportRatio: number;
}

export function useResizablePanel(options: UseResizablePanelOptions) {
  function clampHeight(height: number): number {
    return Math.min(
      Math.max(height, options.minHeight),
      Math.round(window.innerHeight * options.maxViewportRatio),
    );
  }

  function savedHeight(): number {
    try {
      const raw = Number(localStorage.getItem(options.storageKey));
      return Number.isFinite(raw) && raw > 0 ? clampHeight(raw) : options.defaultHeight;
    } catch {
      return options.defaultHeight;
    }
  }

  const panelHeight = ref(savedHeight());
  let finishDrag: (() => void) | null = null;
  onScopeDispose(() => finishDrag?.());

  function onResizeStart(event: PointerEvent) {
    if (!event.isPrimary || event.button !== 0) return;
    finishDrag?.();
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = panelHeight.value;
    const pointerId = event.pointerId;
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(pointerId);
    const onMove = (moveEvent: PointerEvent) => {
      if (moveEvent.pointerId !== pointerId) return;
      panelHeight.value = clampHeight(startHeight + (startY - moveEvent.clientY));
    };
    const onUp = (upEvent: PointerEvent) => {
      if (upEvent.pointerId !== pointerId) return;
      cleanup();
      try {
        localStorage.setItem(options.storageKey, String(panelHeight.value));
      } catch {
        // Storage may be unavailable; the current drag still applied.
      }
    };
    const cleanup = () => {
      finishDrag = null;
      handle.removeEventListener('pointermove', onMove);
      handle.removeEventListener('pointerup', onUp);
      handle.removeEventListener('pointercancel', onUp);
      handle.removeEventListener('lostpointercapture', onUp);
      if (handle.hasPointerCapture(pointerId)) handle.releasePointerCapture(pointerId);
    };
    finishDrag = cleanup;
    handle.addEventListener('pointermove', onMove);
    handle.addEventListener('pointerup', onUp);
    handle.addEventListener('pointercancel', onUp);
    handle.addEventListener('lostpointercapture', onUp);
  }

  return { panelHeight, onResizeStart };
}
