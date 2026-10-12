// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, h, nextTick, shallowRef, type App } from 'vue';
import type { HashResult, ImageHashJob } from '../composables/useImageHash';

const state = vi.hoisted(() => ({ start: vi.fn(), cancel: vi.fn() }));
const job = shallowRef<ImageHashJob>();
const result = shallowRef<HashResult>();
vi.mock('../composables/useImageHash', () => ({
  ALL_HASH_ALGOS: ['md5', 'sha1', 'sha256', 'sm3'],
  useImageHashJob: () => job, useImageHashResult: () => result,
  startImageHash: state.start, cancelImageHash: state.cancel, clearImageHashResult: vi.fn(),
}));
vi.mock('../../../composables/useToast', () => ({ notifyError: vi.fn() }));
import ImageHashPanel from './ImageHashPanel.vue';

let app: App;
let host: HTMLDivElement;
beforeEach(() => {
  job.value = undefined;
  result.value = undefined;
  vi.clearAllMocks();
  host = document.createElement('div');
  document.body.append(host);
  app = createApp({ render: () => h(ImageHashPanel, { imageId: 'disk-one', sizeBytes: 1024 }) });
  app.mount(host);
});
afterEach(() => { app.unmount(); host.remove(); });

describe('compact hash card', () => {
  it('passes the selected algorithms to the compute action', async () => {
    host.querySelectorAll<HTMLInputElement>('input')[0].click();
    await nextTick();
    host.querySelector<HTMLButtonElement>('.hash-btn')!.click();
    expect(state.start).toHaveBeenCalledWith('disk-one', ['sha1', 'sha256', 'sm3']);
    for (const input of host.querySelectorAll<HTMLInputElement>('input:checked')) input.click();
    await nextTick();
    expect(host.querySelector<HTMLButtonElement>('.hash-btn')?.disabled).toBe(true);
  });

  it('shows the running job with its own algorithms and lets it be cancelled', async () => {
    job.value = { imageId: 'disk-one', taskId: 'task', algorithms: ['md5'], bytesDone: 512, bytesTotal: 1024, cancelling: false };
    await nextTick();
    expect(host.querySelector('.hash-progress')?.textContent).toContain('50.0%');
    expect(host.querySelector('.hash-selection')?.textContent).toBe('MD5');
    expect([...host.querySelectorAll<HTMLInputElement>('input:checked')]).toHaveLength(1);
    expect([...host.querySelectorAll<HTMLInputElement>('input')].every(input => input.disabled)).toBe(true);
    host.querySelector<HTMLButtonElement>('.hash-btn')!.click();
    expect(state.cancel).toHaveBeenCalledWith('disk-one');
  });

  it('lists only computed digests next to the read-fault warning', async () => {
    result.value = { md5: '0123456789abcdef', sha1: null, sha256: null, sm3: null,
      read_faults_detected: true, synthetic_bytes: 512, bad_regions: [{ offset: 0, len: 512 }] };
    await nextTick();
    expect([...host.querySelectorAll('.hash-row')].map(row => row.textContent?.trim())).toEqual(['MD50123456789abcdef']);
    expect(host.querySelector('details')).toBeNull();
    expect(host.querySelector('.hash-btn')?.textContent).toContain('重新计算');
    expect(host.querySelector('.hash-fault-warning')?.textContent).toContain('512 B');
  });
});
