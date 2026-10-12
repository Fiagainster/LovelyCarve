// @vitest-environment happy-dom
import { afterEach, expect, it, vi } from 'vitest';
import { createApp, h, nextTick, type App } from 'vue';
import TimelineHeatmap from './TimelineHeatmap.vue';
import TimelineHistogram from './TimelineHistogram.vue';

let app: App;
afterEach(() => { app?.unmount(); document.body.replaceChildren(); });
function key(element: HTMLElement, value: string) {
  element.dispatchEvent(new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true }));
}
it('provides one calendar tab stop and moves a week with the right arrow', async () => {
  const selected = vi.fn(); const host = document.createElement('div'); document.body.append(host);
  app = createApp({ render: () => h(TimelineHeatmap, { year: 2026, buckets: [], tzHours: 8, selection: null, onSelect: selected }) });
  app.mount(host); await nextTick();
  const days = [...host.querySelectorAll<HTMLButtonElement>('.day')];
  expect(days.filter(day => day.tabIndex === 0)).toHaveLength(1);
  days[0].focus(); key(days[0], 'ArrowRight'); await nextTick();
  expect(document.activeElement).toBe(days[7]);
  expect(days.filter(day => day.tabIndex === 0)).toEqual([days[7]]);
  days[7].click(); expect(selected).toHaveBeenCalledWith('2026-01-08');
  key(days[7], 'End'); await nextTick(); expect(document.activeElement).toBe(days.at(-1));
  key(days.at(-1)!, 'Home'); await nextTick(); expect(document.activeElement).toBe(days[0]);
});
it('moves between trend buckets with arrows without selecting until activation', async () => {
  const selected = vi.fn(); const host = document.createElement('div'); document.body.append(host);
  const buckets = [{ date_str: '2025', count: 2 }, { date_str: '2026', count: 3 }];
  app = createApp({ render: () => h(TimelineHistogram, { buckets, unit: 'year', tzHours: 8, onSelect: selected }) });
  app.mount(host); await nextTick();
  const bars = [...host.querySelectorAll<HTMLButtonElement>('.tl-bar')];
  bars[0].focus(); key(bars[0], 'ArrowRight'); await nextTick();
  expect(document.activeElement).toBe(bars[1]); expect(selected).not.toHaveBeenCalled();
  expect(bars.filter(bar => bar.tabIndex === 0)).toEqual([bars[1]]);
  bars[1].click(); expect(selected).toHaveBeenCalledWith(buckets[1]);
  key(bars[1], 'Home'); await nextTick(); expect(document.activeElement).toBe(bars[0]);
});
