// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createApp, h, nextTick, ref, type App } from 'vue';
import FileDetailFields, { type FileDetailField } from './FileDetailFields.vue';

const notifyError = vi.hoisted(() => vi.fn());
vi.mock('../../composables/useToast', () => ({ notifyError }));
let app: App | undefined;
function mount(fields: FileDetailField[]) {
  const rows = ref(fields);
  const host = document.createElement('div');
  document.body.append(host);
  app = createApp({ render: () => h(FileDetailFields, { fields: rows.value }) });
  app.mount(host);
  return rows;
}
afterEach(() => {
  app?.unmount();
  document.body.replaceChildren();
  vi.restoreAllMocks();
  notifyError.mockClear();
});

describe('shared detail fields', () => {
  it('preserves zero offsets, false flags and full multiline values; only missing values use a dash', () => {
    mount([
      { label: '偏移', value: 0, copyable: true },
      { label: '标记', value: false },
      { label: '摘要', value: '第一行\n<script>原始证据</script>' },
      { label: '缺失', value: null, copyable: true },
      { label: '空值', value: '', copyable: true },
    ]);
    expect([...document.querySelectorAll('.field-value')].map(element => element.textContent)).toEqual([
      '0', 'false', '第一行\n<script>原始证据</script>', '—', '—',
    ]);
    expect(document.querySelector('script')).toBeNull();
    expect(document.querySelectorAll('button')).toHaveLength(1);
  });

  it('copies the complete value and drops stale feedback after switching records', async () => {
    let finishCopy!: () => void;
    const copy = vi.spyOn(navigator.clipboard, 'writeText').mockImplementation(() => new Promise(resolve => { finishCopy = resolve; }));
    const value = '/证据/很长的目录/'.repeat(20) + '原始文件.pdf';
    const rows = mount([{ label: '路径', value, copyable: true }]);
    document.querySelector('button')!.click();
    expect(copy).toHaveBeenCalledWith(value);
    rows.value = [{ label: '路径', value: '/新的文件.pdf', copyable: true }];
    await nextTick();
    finishCopy();
    await nextTick();
    expect(document.querySelector('[role="status"]')?.textContent).toBe('');
    copy.mockResolvedValue();
    document.querySelector('button')!.click();
    await nextTick(); await nextTick();
    expect(document.querySelector('[role="status"]')?.textContent).toBe('已复制');
  });

  it('reports clipboard errors without claiming the value was copied', async () => {
    vi.spyOn(navigator.clipboard, 'writeText').mockRejectedValue(new Error('clipboard unavailable'));
    mount([{ label: 'MD5', value: 'abc', copyable: true }]);
    document.querySelector('button')!.click();
    await nextTick(); await nextTick();
    expect(notifyError).toHaveBeenCalledWith('复制失败', 'Error: clipboard unavailable');
    expect(document.querySelector('[role="status"]')?.textContent).toBe('');
  });
});
