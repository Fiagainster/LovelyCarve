// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { effectScope } from 'vue';
import { useResizablePanel } from './useResizablePanel';

function pointer(type: string, pointerId: number, clientY: number) {
  const event = new Event(type);
  Object.assign(event, { pointerId, clientY });
  return event;
}

function setup() {
  const scope = effectScope();
  const panel = scope.run(() => useResizablePanel({ storageKey: 'test.panel', defaultHeight: 250, minHeight: 180, maxViewportRatio: 0.5 }))!;
  const handle = document.createElement('div');
  let captured = false;
  handle.setPointerCapture = vi.fn(() => { captured = true; });
  handle.hasPointerCapture = vi.fn(() => captured);
  handle.releasePointerCapture = vi.fn(() => { captured = false; });
  panel.onResizeStart({ isPrimary: true, button: 0, clientY: 400, pointerId: 7, currentTarget: handle, preventDefault: vi.fn() } as unknown as PointerEvent);
  return { scope, panel, handle };
}

beforeEach(() => { localStorage.clear(); vi.stubGlobal('innerHeight', 1000); });

describe('preview panel resize ownership', () => {
  it('ignores other pointers, clamps the height and saves only when the drag ends', () => {
    const { scope, panel, handle } = setup();
    handle.dispatchEvent(pointer('pointermove', 8, 0)); expect(panel.panelHeight.value).toBe(250);
    handle.dispatchEvent(pointer('pointermove', 7, 0)); expect(panel.panelHeight.value).toBe(500);
    handle.dispatchEvent(pointer('pointerup', 8, 0)); expect(localStorage.getItem('test.panel')).toBeNull();
    handle.dispatchEvent(pointer('pointermove', 7, 900)); expect(panel.panelHeight.value).toBe(180);
    handle.dispatchEvent(pointer('pointerup', 7, 900)); expect(localStorage.getItem('test.panel')).toBe('180');
    expect(handle.releasePointerCapture).toHaveBeenCalledWith(7);
    handle.dispatchEvent(pointer('pointermove', 7, 0)); expect(panel.panelHeight.value).toBe(180);
    scope.stop(); vi.unstubAllGlobals();
  });

  it('releases capture and listeners on disposal without saving an unfinished drag', () => {
    const { scope, panel, handle } = setup();
    handle.dispatchEvent(pointer('pointermove', 7, 350)); expect(panel.panelHeight.value).toBe(300);
    scope.stop();
    expect(handle.releasePointerCapture).toHaveBeenCalledWith(7);
    handle.dispatchEvent(pointer('pointermove', 7, 0)); handle.dispatchEvent(pointer('pointerup', 7, 0));
    expect(panel.panelHeight.value).toBe(300); expect(localStorage.getItem('test.panel')).toBeNull();
    vi.unstubAllGlobals();
  });
});
