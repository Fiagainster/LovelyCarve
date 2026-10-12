// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { ref, nextTick, defineComponent, createApp, h, KeepAlive } from 'vue';
import { useVirtualList } from './useVirtualList';

// Minimal headless harness: useVirtualList relies on onBeforeUnmount + watch,
// which require an active component instance. We wrap in a tiny runner.
function withComponent<T>(factory: () => T): Promise<{ result: T; dispose: () => void }> {
  return new Promise((resolve) => {
    // Lazy import to avoid top-level cost; vitest picks up vue fine
    import('vue').then(({ defineComponent, createApp, h }) => {
      let captured: T;
      const Comp = defineComponent({
        setup() {
          captured = factory();
          return () => h('div');
        },
      });
      const app = createApp(Comp);
      const host = document.createElement('div');
      app.mount(host);
      // Defer to let onMounted run; setup already captured the value.
      queueMicrotask(() => resolve({ result: captured!, dispose: () => app.unmount() }));
    });
  });
}

function makeContainer(clientHeight: number) {
  const el = document.createElement('div');
  // jsdom doesn't implement layout; stub the readonly props.
  Object.defineProperty(el, 'clientHeight', { value: clientHeight, configurable: true });
  el.scrollTop = 0;
  return el;
}

describe('useVirtualList', () => {
  let rafCb: FrameRequestCallback | null = null;
  let origRaf: typeof requestAnimationFrame;
  let origCancel: typeof cancelAnimationFrame;

  beforeEach(() => {
    origRaf = globalThis.requestAnimationFrame;
    origCancel = globalThis.cancelAnimationFrame;
    globalThis.requestAnimationFrame = ((cb: FrameRequestCallback) => {
      rafCb = cb;
      return 1 as unknown as number;
    }) as typeof requestAnimationFrame;
    globalThis.cancelAnimationFrame = (() => {
      rafCb = null;
    }) as typeof cancelAnimationFrame;
    // Stub ResizeObserver
    (globalThis as any).ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
  });

  afterEach(() => {
    globalThis.requestAnimationFrame = origRaf;
    globalThis.cancelAnimationFrame = origCancel;
    rafCb = null;
  });

  it('clamps a deep scroll when filtering shrinks or clears the list', async () => {
    const items = ref(Array.from({ length: 1000 }, (_, index) => index));
    const element = makeContainer(360);
    element.scrollTop = 18_000;
    const { result: list, dispose } = await withComponent(() =>
      useVirtualList({ items, itemHeight: 36, containerRef: ref(element) }),
    );
    items.value = [1, 2, 3];
    await nextTick();
    expect(list.visibleItems.value.map(row => row.item)).toEqual([1, 2, 3]);
    expect([element.scrollTop, list.paddingTop.value, list.paddingBottom.value]).toEqual([0, 0, 0]);
    items.value = [];
    await nextTick();
    expect(list.range.value).toEqual({ start: 0, end: 0 });
    dispose();
  });

  it('returns an empty window when items are empty', async () => {
    const items = ref<number[]>([]);
    const container = ref<HTMLElement | null>(makeContainer(500));

    const { result: vl, dispose } = await withComponent(() =>
      useVirtualList({ items, itemHeight: 22, containerRef: container }),
    );

    expect(vl.visibleItems.value).toHaveLength(0);
    expect(vl.totalHeight.value).toBe(0);
    expect(vl.paddingTop.value).toBe(0);
    expect(vl.paddingBottom.value).toBe(0);
    dispose();
  });

  it('renders only the visible window for a large list', async () => {
    const items = ref(Array.from({ length: 10_000 }, (_, i) => i));
    const container = ref<HTMLElement | null>(makeContainer(220)); // ~10 rows @22px

    const { result: vl, dispose } = await withComponent(() =>
      useVirtualList({ items, itemHeight: 22, containerRef: container, overscan: 4 }),
    );

    await nextTick();
    // With viewport=220, itemHeight=22 → ~10 visible, + 2×overscan(4) = 18 rows max.
    expect(vl.visibleItems.value.length).toBeLessThanOrEqual(20);
    expect(vl.visibleItems.value.length).toBeGreaterThan(0);
    // First window starts at 0.
    expect(vl.visibleItems.value[0]?.index).toBe(0);
    // Total height tracks all items.
    expect(vl.totalHeight.value).toBe(10_000 * 22);
    dispose();
  });

  it('shifts the window when scrollTop advances', async () => {
    const items = ref(Array.from({ length: 5_000 }, (_, i) => i));
    const el = makeContainer(220);
    const container = ref<HTMLElement | null>(el);

    const { result: vl, dispose } = await withComponent(() =>
      useVirtualList({ items, itemHeight: 22, containerRef: container, overscan: 2 }),
    );
    await nextTick();

    // Scroll to row ~500 (index 500 → scrollTop = 500 * 22 = 11000)
    el.scrollTop = 11_000;
    el.dispatchEvent(new Event('scroll'));
    // Flush the rAF
    if (rafCb) rafCb(0);
    await nextTick();

    const firstIdx = vl.visibleItems.value[0]?.index ?? -1;
    expect(firstIdx).toBeGreaterThanOrEqual(498); // near 500 - overscan
    expect(firstIdx).toBeLessThanOrEqual(500);
    dispose();
  });

  it('scrollToIndex updates container scrollTop to center the row', async () => {
    const items = ref(Array.from({ length: 1_000 }, (_, i) => i));
    const el = makeContainer(440); // 20 rows @22px, center offset = 220
    const container = ref<HTMLElement | null>(el);

    const { result: vl, dispose } = await withComponent(() =>
      useVirtualList({ items, itemHeight: 22, containerRef: container }),
    );
    await nextTick();

    vl.scrollToIndex(100, 'center');
    // target 100*22 = 2200; center → 2200 - 440/2 + 22/2 = 2200 - 220 + 11 = 1991
    expect(el.scrollTop).toBe(1991);
    dispose();
  });

  // 服务端分页(全显 infinite 模型):滚动高度按服务端总数撑开,已加载行之外
  // 的区间留白但仍计入下垫,总高度恒等于 total × 行高;range 越过已加载行
  // 时宿主据此补页。
  it('virtualCount sizes the scroller by the server total and leaves unloaded rows blank', async () => {
    const items = ref(Array.from({ length: 100 }, (_, i) => i));
    const total = ref(10_000);
    const el = makeContainer(220);
    const container = ref<HTMLElement | null>(el);

    const { result: vl, dispose } = await withComponent(() =>
      useVirtualList({ items, itemHeight: 22, containerRef: container, overscan: 2, virtualCount: total }),
    );
    await nextTick();
    expect(vl.totalHeight.value).toBe(10_000 * 22);
    // 顶部窗口全部已加载:正常产出行。
    expect(vl.visibleItems.value[0]?.index).toBe(0);
    expect(vl.paddingTop.value + vl.visibleItems.value.length * 22 + vl.paddingBottom.value).toBe(10_000 * 22);

    // 跳到第 5000 行:区间落在未加载段,不产出行,但上下垫仍撑满总高度。
    el.scrollTop = 5_000 * 22;
    el.dispatchEvent(new Event('scroll'));
    if (rafCb) rafCb(0);
    await nextTick();
    expect(vl.visibleItems.value).toHaveLength(0);
    expect(vl.range.value.end).toBeGreaterThan(5_000);
    expect(vl.paddingTop.value + vl.paddingBottom.value).toBe(10_000 * 22);

    // 补页到位后该区间即产出行;总数小于已加载行数时退化为按已加载行数虚拟化。
    items.value = Array.from({ length: 6_000 }, (_, i) => i);
    await nextTick();
    expect(vl.visibleItems.value.length).toBeGreaterThan(0);
    total.value = 10;
    await nextTick();
    expect(vl.totalHeight.value).toBe(6_000 * 22);
    dispose();
  });

  // Regression: a host under <KeepAlive> keeps its component instance but its
  // DOM is reparented out and back when toggled. The browser silently resets a
  // reparented scroll container's scrollTop to 0 WITHOUT firing a scroll event,
  // so the reactive scrollTop would otherwise stay stale and leave paddingTop
  // pinned to the old position — a blank gap at the top with the first rows
  // pushed out of view. onActivated must re-read the real DOM scrollTop.
  it('re-syncs scrollTop to the real DOM on KeepAlive re-activation', async () => {
    const items = ref(Array.from({ length: 1_000 }, (_, i) => i));
    const el = makeContainer(220);
    const container = ref<HTMLElement | null>(el);
    let vl!: ReturnType<typeof useVirtualList<number>>;

    const show = ref(true);
    const Child = defineComponent({
      setup() {
        vl = useVirtualList({ items, itemHeight: 22, containerRef: container, overscan: 2 });
        return () => h('div');
      },
    });
    const Parent = defineComponent({
      setup() {
        return () => h(KeepAlive, null, { default: () => (show.value ? h(Child) : null) });
      },
    });
    const app = createApp(Parent);
    app.mount(document.createElement('div'));
    await nextTick();

    // Scroll the body down: the virtual window advances and paddingTop grows.
    el.scrollTop = 2_200;
    el.dispatchEvent(new Event('scroll'));
    if (rafCb) rafCb(0);
    await nextTick();
    expect(vl.paddingTop.value).toBeGreaterThan(0);

    // Switch the panel away → KeepAlive deactivates (DOM parked, instance kept).
    show.value = false;
    await nextTick();
    // Browser silently resets the reparented container's scrollTop, no scroll event.
    el.scrollTop = 0;

    // Switch back → onActivated re-reads the real scrollTop (0), so the window
    // snaps back to the top instead of leaving a stale blank spacer.
    show.value = true;
    await nextTick();
    expect(vl.paddingTop.value).toBe(0);
    expect(vl.visibleItems.value[0]?.index).toBe(0);

    app.unmount();
  });
});
