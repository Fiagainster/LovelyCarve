// @vitest-environment happy-dom
import { createApp, h, nextTick, ref, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import EncryptedVolumeUnlockDialog from './EncryptedVolumeUnlockDialog.vue';
import type { ImageInfo } from '../../../lib/ipc/image';

const rpc = vi.hoisted(() => ({ start: vi.fn() }));
vi.mock('../composables/useEncryptedVolumeUnlocks', () => ({ startEncryptedVolumeUnlock: rpc.start }));
const info = {
  id: 'source',
  partitions: [{ id: 1, kind: 'ntfs', label: 'System' }, { id: 2, kind: 'luks', label: 'Linux' }],
} as ImageInfo;
let app: App;
const open = ref(true);
const partitionId = ref<number | null>(null);
const closed = vi.fn(() => { open.value = false; });

/** The newest dialog: one that just closed may still be leaving. */
function dialog(): HTMLElement | null {
  return [...document.querySelectorAll<HTMLElement>('[role=dialog]')].pop() ?? null;
}
async function settle() {
  for (let i = 0; i < 4; i += 1) await nextTick();
}
async function value(selector: string, text: string) {
  const element = dialog()!.querySelector<HTMLInputElement>(selector)!;
  element.value = text;
  element.dispatchEvent(new Event('input', { bubbles: true }));
  await nextTick();
}
function selected(field: number): string | undefined {
  // The trigger also holds every other label (hidden) to keep its width.
  const trigger = dialog()!.querySelectorAll<HTMLButtonElement>('button[role=combobox]')[field];
  return trigger.querySelector('.ui-select-value > :not(.ui-select-sizer)')?.textContent?.trim();
}
async function choose(field: number, label: string) {
  dialog()!.querySelectorAll<HTMLButtonElement>('button[role=combobox]')[field].click();
  await settle();
  [...document.querySelectorAll<HTMLElement>('[role=option]')].find(option => option.textContent === label)!.click();
  await nextTick();
}
async function submit() {
  dialog()!.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
  await settle();
}
beforeEach(async () => {
  vi.clearAllMocks();
  rpc.start.mockReturnValue(true);
  open.value = true;
  partitionId.value = null;
  app = createApp({
    render: () => h(EncryptedVolumeUnlockDialog, {
      open: open.value, info, partitionId: partitionId.value, onClose: closed,
    }),
  });
  app.mount(document.createElement('div'));
  await settle();
});
afterEach(() => {
  app.unmount();
  // A dialog that was closing stays in its leave transition; do not let the
  // next test find it.
  document.body.innerHTML = '';
});

describe('encrypted volume unlock dialog', () => {
  it('forwards partition and VeraCrypt options and closes immediately after background submission', async () => {
    let sent: unknown;
    rpc.start.mockImplementation((source, options) => { sent = structuredClone({ imageId: source.id, options }); return true; });
    await choose(0, 'VeraCrypt');
    await choose(1, '分区 2 · Linux');
    await value('input[type=password]', 'secret');
    await value('input[type=number]', '1');
    dialog()!.querySelector<HTMLInputElement>('input[type=checkbox]')!.click();
    await nextTick();
    await submit();
    expect(sent).toMatchObject({ imageId: 'source', options: { scheme: 'veracrypt', password: 'secret', partitionId: 2, pim: 1, hidden: true } });
    expect(closed).toHaveBeenCalled();
    open.value = true;
    await settle();
    expect(dialog()!.querySelector<HTMLInputElement>('input[type=password]')!.value).toBe('');
  });

  it('keeps the dialog open when an existing background job rejects submission and clears credentials', async () => {
    rpc.start.mockReturnValue(false);
    await value('input[type=password]', 'wrong');
    await submit();
    expect(closed).not.toHaveBeenCalled();
    expect(dialog()!.querySelector<HTMLInputElement>('input[type=password]')!.value).toBe('');
  });

  it('starts from the LUKS partition it was opened for and forgets the password on close', async () => {
    open.value = false;
    await settle();
    partitionId.value = 2;
    open.value = true;
    await settle();
    expect(selected(0)).toBe('LUKS1 / LUKS2');
    expect(dialog()!.querySelectorAll('button[role=combobox]')).toHaveLength(1);
    expect(dialog()!.querySelector('.ui-modal-title')?.textContent).toBe('解锁分区 2');
    expect(dialog()!.querySelector('.ui-modal-subtitle, .ui-alert, .hint')).toBeNull();
    await value('input[type=password]', 'typed');

    dialog()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();
    expect(closed).toHaveBeenCalled();
    open.value = true;
    await settle();
    expect(dialog()!.querySelector<HTMLInputElement>('input[type=password]')!.value).toBe('');
  });

  it('submits the fixed partition without exposing a region chooser', async () => {
    open.value = false;
    await settle();
    partitionId.value = 2;
    open.value = true;
    await settle();
    await value('input[type=password]', 'secret');
    await submit();
    expect(rpc.start).toHaveBeenCalledWith(info, { scheme: 'luks', password: 'secret', partitionId: 2 });
  });

  it('submits LUKS for the whole image and leaves cancellation to the task center', async () => {
    await value('input[type=password]', 'secret');
    await submit();
    expect(rpc.start).toHaveBeenCalledWith(info, { scheme: 'luks', password: 'secret' });
    expect(closed).toHaveBeenCalledOnce();
  });
});
