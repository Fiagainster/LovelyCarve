import { ref, computed, onActivated, onBeforeUnmount, watch, type Ref, type ComputedRef } from 'vue';

export interface UseVirtualListOptions<T> {
  items: Ref<T[]> | ComputedRef<T[]>;
  itemHeight: number;
  containerRef: Ref<HTMLElement | null>;
  overscan?: number;
  /** 服务端分页(infinite 模型)下的总行数:滚动高度与可视区间按它计算,
   *  而不是按已加载的 `items.length`,这样滚动条一开始就反映全部结果;
   *  区间内尚未加载的下标不产出行(留白),由宿主监听 `range` 补页。
   *  省略或小于 `items.length` 时退化为普通的按已加载行数虚拟化。 */
  virtualCount?: Ref<number> | ComputedRef<number>;
}

export interface VirtualItem<T> {
  item: T;
  index: number;
}

export function useVirtualList<T>(opts: UseVirtualListOptions<T>) {
  const overscan = opts.overscan ?? 8;
  const scrollTop = ref(0);
  const viewportHeight = ref(0);

  let rafId: number | null = null;
  let attachedEl: HTMLElement | null = null;
  let ro: ResizeObserver | null = null;

  const flushScroll = () => {
    rafId = null;
    if (attachedEl) scrollTop.value = attachedEl.scrollTop;
  };

  const onScroll = () => {
    if (rafId !== null) return;
    rafId = requestAnimationFrame(flushScroll);
  };

  const measure = () => {
    if (attachedEl) viewportHeight.value = attachedEl.clientHeight;
  };

  const detach = () => {
    if (attachedEl) {
      attachedEl.removeEventListener('scroll', onScroll);
      attachedEl = null;
    }
    if (ro) {
      ro.disconnect();
      ro = null;
    }
  };

  const attach = (el: HTMLElement) => {
    detach();
    attachedEl = el;
    el.addEventListener('scroll', onScroll, { passive: true });
    scrollTop.value = el.scrollTop;
    measure();
    if (typeof ResizeObserver !== 'undefined') {
      ro = new ResizeObserver(measure);
      ro.observe(el);
    }
  };

  watch(
    opts.containerRef,
    (el) => {
      if (el) attach(el);
      else detach();
    },
    { immediate: true, flush: 'post' },
  );

  // 宿主组件在 <KeepAlive> 下被切走再切回时，Vue 把缓存的 DOM 搬回文档，浏览器
  // 会把滚动容器的 scrollTop 静默重置为 0（不派发 scroll 事件）；停用期间的窗口
  // resize 也不会被 ResizeObserver 补到。此时响应式 scrollTop / viewportHeight
  // 仍停在切走前的旧值，paddingTop 会按旧滚动位置撑出一段空白、把头部几行顶出
  // 视口（表现为「列表从第 N 行才开始、顶部留白」）。激活时按真实 DOM 重新同步
  // 一次。containerRef 不变则上面的 watch 不会重跑 attach，必须单独在这里补同步；
  // 不在 KeepAlive 下时 onActivated 永不触发，无副作用。
  onActivated(() => {
    if (!attachedEl) return;
    scrollTop.value = attachedEl.scrollTop;
    measure();
  });

  onBeforeUnmount(() => {
    detach();
    if (rafId !== null) cancelAnimationFrame(rafId);
  });

  const loadedCount = computed(() => opts.items.value.length);
  const itemCount = computed(() =>
    Math.max(loadedCount.value, opts.virtualCount?.value ?? 0),
  );
  const totalHeight = computed(() => itemCount.value * opts.itemHeight);

  const boundedScrollTop = computed(() => Math.min(
    scrollTop.value,
    Math.max(0, totalHeight.value - (viewportHeight.value || opts.itemHeight * 40)),
  ));
  watch([itemCount, viewportHeight], () => {
    const top = boundedScrollTop.value;
    if (scrollTop.value !== top) {
      scrollTop.value = top;
      if (attachedEl) attachedEl.scrollTop = top;
    }
  }, { flush: 'sync' });

  const range = computed(() => {
    const count = itemCount.value;
    if (count === 0) return { start: 0, end: 0 };
    // If viewport not measured yet, render a small head so SSR/first paint still shows content.
    const vh = viewportHeight.value > 0 ? viewportHeight.value : opts.itemHeight * 40;
    const start = Math.max(0, Math.floor(boundedScrollTop.value / opts.itemHeight) - overscan);
    const visibleCount = Math.ceil(vh / opts.itemHeight);
    const end = Math.min(count, start + visibleCount + overscan * 2);
    return { start, end };
  });

  const visibleItems = computed<VirtualItem<T>[]>(() => {
    const { start, end } = range.value;
    const items = opts.items.value;
    // 区间可能越过已加载行(服务端分页的留白段),只产出真实存在的行。
    const loadedEnd = Math.min(end, items.length);
    const result: VirtualItem<T>[] = new Array(Math.max(0, loadedEnd - start));
    for (let i = start; i < loadedEnd; i++) {
      result[i - start] = { item: items[i], index: i };
    }
    return result;
  });

  const paddingTop = computed(() => range.value.start * opts.itemHeight);
  // 下垫按「实际产出的最后一行」算,而不是按区间末尾:区间越过已加载行时
  // 留白段也计入下垫,总高度始终等于 itemCount × 行高,滚动条不会在补页
  // 前后跳动。
  const paddingBottom = computed(() => {
    const renderedEnd = Math.max(range.value.start, Math.min(range.value.end, loadedCount.value));
    return Math.max(0, (itemCount.value - renderedEnd) * opts.itemHeight);
  });

  function scrollToIndex(index: number, align: 'start' | 'center' | 'end' = 'center') {
    const el = attachedEl;
    if (!el) return;
    const clamped = Math.max(0, Math.min(index, itemCount.value - 1));
    const target = clamped * opts.itemHeight;
    let top = target;
    if (align === 'center') top = target - el.clientHeight / 2 + opts.itemHeight / 2;
    else if (align === 'end') top = target - el.clientHeight + opts.itemHeight;
    el.scrollTop = Math.max(0, Math.min(top, totalHeight.value - el.clientHeight));
    scrollTop.value = el.scrollTop;
  }

  function scrollToTop() {
    scrollTop.value = 0;
    if (attachedEl) attachedEl.scrollTop = 0;
  }

  return {
    visibleItems,
    paddingTop,
    paddingBottom,
    totalHeight,
    range,
    scrollToIndex,
    scrollToTop,
  };
}
