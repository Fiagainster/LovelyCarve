// @vitest-environment happy-dom
import { createApp, nextTick, ref, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import AppLogPanel from './AppLogPanel.vue';
import type { ConsoleEntry } from '../../../composables/useConsoleCapture';

const entries = ref<ConsoleEntry[]>([]);
const copyAll = vi.fn();
const clear = vi.fn(() => { entries.value = []; });
vi.mock('../../../composables/useConsoleCapture', () => ({ useConsoleCapture: () => ({ entries, copyAll, clear }) }));

let app: App;
let host: HTMLElement;
function entry(id: number, level: ConsoleEntry['level'], text: string): ConsoleEntry {
  return { id, level, timestamp: Date.parse('2026-10-12T01:02:03.123Z'), parts: [text], text };
}
function button(label: string) {
  return [...host.querySelectorAll<HTMLButtonElement>('button')].find(element => element.textContent?.trim().startsWith(label))!;
}
async function settle() { await nextTick(); await nextTick(); }
beforeEach(() => {
  vi.clearAllMocks();
  copyAll.mockReset();
  entries.value = [entry(1, 'info', '镜像读取完成'), entry(2, 'error', '导出失败：路径不存在')];
  host = document.createElement('div'); document.body.append(host);
  app = createApp(AppLogPanel); app.mount(host);
});
afterEach(() => { app.unmount(); host.remove(); });

describe('settings session logs', () => {
  it('shows captured output, searches it and filters by severity without deleting entries', async () => {
    expect(host.querySelectorAll('.log-entry').length).toBe(2);
    const input = host.querySelector<HTMLInputElement>('[aria-label="搜索日志"]')!;
    input.value = '导出'; input.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    expect([...host.querySelectorAll('.log-text')].map(element => element.textContent)).toEqual(['导出失败：路径不存在']);
    button('错误').click();
    await settle();
    expect(button('错误').getAttribute('aria-pressed')).toBe('false');
    expect(host.querySelector('[role="status"]')?.textContent).toContain('没有符合筛选条件');
    expect(entries.value.length).toBe(2);
    button('错误').click();
    await settle();
    expect(host.querySelectorAll('.log-entry').length).toBe(1);
  });

  it('reports clipboard errors inline, allows retry and clears the collected session', async () => {
    copyAll.mockRejectedValueOnce(new Error('剪贴板不可用')).mockResolvedValueOnce(undefined);
    button('复制全部').click(); await settle();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('剪贴板不可用');
    expect(button('复制全部').disabled).toBe(false);
    button('复制全部').click(); await settle();
    expect(host.querySelector('[role="status"]')?.textContent).toBe('已复制全部日志。');
    button('清空').click(); await settle();
    expect(clear).toHaveBeenCalledOnce();
    expect(host.textContent).toContain('暂无日志');
    expect(button('复制全部').disabled).toBe(true);
  });

  it('avoids duplicate copies and waits for clipboard completion before allowing clear', async () => {
    let finish!: () => void;
    copyAll.mockImplementation(() => new Promise<void>(resolve => { finish = resolve; }));
    button('复制全部').click(); await settle();
    expect([button('复制全部').disabled, button('清空').disabled]).toEqual([true, true]);
    button('复制全部').click();
    expect(copyAll).toHaveBeenCalledOnce();
    finish(); await settle();
    expect(button('清空').disabled).toBe(false);
  });

  it('adds new output without forcing scrolling while auto scroll is disabled', async () => {
    const toggle = host.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    toggle.checked = false; toggle.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    const body = host.querySelector<HTMLElement>('.log-body')!;
    Object.defineProperty(body, 'scrollHeight', { configurable: true, value: 1200 });
    body.scrollTop = 40;
    entries.value.push(entry(3, 'warn', '镜像路径发生变化'));
    await settle();
    expect(body.scrollTop).toBe(40);
    expect(host.querySelectorAll('.log-entry').length).toBe(3);
    toggle.checked = true; toggle.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    expect(body.scrollTop).toBe(1200);
  });

  it('keeps the reading position when the user scrolls up with auto scroll enabled', async () => {
    await settle();
    const body = host.querySelector<HTMLElement>('.log-body')!;
    Object.defineProperties(body, {
      scrollHeight: { configurable: true, value: 1200 },
      clientHeight: { configurable: true, value: 200 },
    });
    body.scrollTop = 100;
    body.dispatchEvent(new Event('scroll'));
    entries.value.push(entry(3, 'warn', '新的运行日志'));
    await settle();
    expect(body.scrollTop).toBe(100);
    expect(host.querySelectorAll('.log-entry').length).toBe(3);
  });
});
