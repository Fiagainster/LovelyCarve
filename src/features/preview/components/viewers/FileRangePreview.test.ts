// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, h, nextTick, reactive, type App } from 'vue';
import FileRangePreview from './FileRangePreview.vue';

const ipc = vi.hoisted(() => ({ read: vi.fn() }));
vi.mock('../../../../lib/ipc/fs/read', () => ({ fsReadRange: ipc.read }));
let app: App;
let host: HTMLDivElement;
let props: { imageId: string; nodeId: string; size: number; target: { offset: number; length: number } };
async function settle() { for (let index = 0; index < 8; index++) await nextTick(); }
async function mount() {
  app = createApp({ render: () => h(FileRangePreview, props) });
  app.mount(host); await settle();
}
async function click(label: string) {
  host.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!.click(); await settle();
}
beforeEach(() => {
  vi.resetAllMocks();
  Element.prototype.scrollIntoView = vi.fn();
  props = reactive({ imageId: 'disk-a', nodeId: 'file', size: 8_000_000, target: { offset: 4_194_308, length: 3 } });
  ipc.read.mockImplementation(async ({ length }) => new Uint8Array(length).fill(65).buffer);
  host = document.createElement('div'); document.body.append(host);
});
afterEach(() => { app?.unmount(); host.remove(); });

describe('search byte location preview', () => {
  it('reads a bounded range beyond 1 MiB and highlights the exact match at absolute offsets', async () => {
    await mount();
    expect(ipc.read).toHaveBeenCalledExactlyOnceWith({ imageId: 'disk-a', nodeId: 'file', offset: 4_194_304, length: 4096 });
    expect([...host.querySelectorAll('.byte-cells .hit')].map(cell => cell.getAttribute('data-offset'))).toEqual(['4194308', '4194309', '4194310']);
    expect(host.querySelector('.hex-row:not(.heading) .offset')?.textContent).toBe('00400000');
  });

  it('can page across a match boundary and return to the original hit', async () => {
    props.target = { offset: 4094, length: 4 };
    await mount();
    expect(host.querySelectorAll('.byte-cells .hit')).toHaveLength(2);
    await click('下一段');
    expect([...host.querySelectorAll('.byte-cells .hit')].map(cell => cell.getAttribute('data-offset'))).toEqual(['4096', '4097']);
    await click('回到命中位置');
    expect(ipc.read).toHaveBeenLastCalledWith(expect.objectContaining({ offset: 0 }));
  });

  it('clamps the final read to EOF and disables forward paging', async () => {
    props.size = 4099; props.target = { offset: 4098, length: 1 };
    await mount();
    expect(ipc.read).toHaveBeenLastCalledWith(expect.objectContaining({ offset: 4096, length: 3 }));
    expect(host.querySelector<HTMLButtonElement>('[aria-label="下一段"]')?.disabled).toBe(true);
    expect(host.querySelectorAll('.byte-cells .hit')).toHaveLength(1);
  });

  it('reports stale out-of-range hits without reading an unrelated byte', async () => {
    props.target.offset = props.size;
    await mount();
    expect(ipc.read).not.toHaveBeenCalled();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('超出文件范围');
  });

  it('reports short reads that do not cover the target', async () => {
    ipc.read.mockResolvedValue(new Uint8Array(2).buffer);
    await mount();
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('未覆盖命中位置');
  });

  it('drops a late read from the previously selected evidence', async () => {
    let finish!: (value: ArrayBuffer) => void;
    ipc.read.mockReturnValueOnce(new Promise<ArrayBuffer>(resolve => { finish = resolve; }));
    await mount();
    props.imageId = 'disk-b';
    props.target = { offset: 8, length: 1 };
    await settle();
    finish(new Uint8Array([255]).buffer);
    await settle();
    expect(host.querySelector('.byte-cells .hit')?.textContent).toBe('41');
    expect([...host.querySelectorAll('.byte-cells > span')].every(cell => cell.textContent === '41')).toBe(true);
  });
});
