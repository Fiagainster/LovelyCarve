// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from 'vitest';
import { createApp, ref, type App } from 'vue';
import { useMarqueeSelection } from './useMarqueeSelection';

const ROW_HEIGHT = 36;
const rows = ref(Array.from({ length: 10 }, (_, index) => ({ id: `r${index}` })));
let app: App | null = null;

function setup(options: { checked: string[]; blocked?: boolean }) {
  const container = document.createElement('div');
  Object.defineProperty(container, 'clientWidth', { value: 300 });
  Object.defineProperty(container, 'clientHeight', { value: 200 });
  container.getBoundingClientRect = () => ({
    left: 0, top: 0, right: 300, bottom: 200, width: 300, height: 200, x: 0, y: 0, toJSON: () => ({}),
  });
  document.body.append(container);
  const containerRef = ref<HTMLElement | null>(container);
  const checkedIds = ref(new Set(options.checked));
  let marquee!: ReturnType<typeof useMarqueeSelection>;
  app = createApp({
    setup() {
      marquee = useMarqueeSelection({
        container: containerRef,
        rows,
        rowHeight: ROW_HEIGHT,
        checkedIds,
        isBlocked: () => options.blocked === true,
      });
      return () => null;
    },
  });
  app.mount(document.createElement('div'));
  container.addEventListener('mousedown', event => marquee.onMarqueeMouseDown(event));
  return { container, checkedIds, marquee };
}

function drag(container: HTMLElement, from: { x: number; y: number }, to: { x: number; y: number }) {
  container.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, clientX: from.x, clientY: from.y }));
  window.dispatchEvent(new MouseEvent('mousemove', { buttons: 1, clientX: to.x, clientY: to.y }));
  window.dispatchEvent(new MouseEvent('mouseup', { button: 0, clientX: to.x, clientY: to.y }));
}

afterEach(() => {
  app?.unmount();
  app = null;
  document.body.replaceChildren();
});

describe('useMarqueeSelection', () => {
  it('flips the rows the box covers relative to the checks at drag start', () => {
    const { container, checkedIds, marquee } = setup({ checked: ['r1', 'r5'] });

    drag(container, { x: 10, y: 10 }, { x: 40, y: 100 });

    // Rows 0–2 are covered: r0/r2 become checked, r1 unchecked; r5 is untouched.
    expect([...checkedIds.value].sort()).toEqual(['r0', 'r2', 'r5']);
    expect(marquee.marqueeRect.value).toBeNull();
    expect(marquee.marqueeJustEnded()).toBe(true);
  });

  it('treats a drag below the threshold as a plain click', () => {
    const { container, checkedIds, marquee } = setup({ checked: ['r1'] });

    drag(container, { x: 10, y: 10 }, { x: 12, y: 12 });

    expect([...checkedIds.value]).toEqual(['r1']);
    expect(marquee.marqueeJustEnded()).toBe(false);
  });

  it('does not start while the listing is blocked', () => {
    const { container, checkedIds } = setup({ checked: [], blocked: true });

    drag(container, { x: 10, y: 10 }, { x: 40, y: 100 });

    expect(checkedIds.value.size).toBe(0);
  });
});
