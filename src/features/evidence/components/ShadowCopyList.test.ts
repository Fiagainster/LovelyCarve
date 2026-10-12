// @vitest-environment happy-dom
import { createApp, h, nextTick, shallowRef, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import ShadowCopyList from './ShadowCopyList.vue';
import type { ShadowCopyInfo, ShadowVolumeInfo } from '../../../lib/ipc/shadow';

let app: App;
let host: HTMLDivElement;
const opened = vi.fn();

function copy(index: number, overrides: Partial<ShadowCopyInfo> = {}): ShadowCopyInfo {
  return {
    index,
    store_id: `store-${index}`,
    shadow_copy_id: `copy-${index}`,
    shadow_copy_set_id: null,
    creation_filetime: 0,
    created_unix_ms: 1_619_890_803_223 + index * 85_000,
    volume_size: 82_771_968,
    snapshot_context: 9,
    attributes: 0x0042_0009,
    persistent: true,
    client_accessible: false,
    operating_machine: 'DESKTOP-01',
    service_machine: 'DESKTOP-01',
    unavailable_reason: null,
    open_image_id: null,
    ...overrides,
  };
}

function volume(shadow_copies: ShadowCopyInfo[], error: string | null = null): ShadowVolumeInfo {
  return { partition_id: 2, volume_size: 82_771_968, shadow_copies, error };
}

const props = shallowRef<{ volume: ShadowVolumeInfo; opening: string | null; error: string | null }>({
  volume: volume([]), opening: null, error: null,
});

async function mount(next: Partial<typeof props.value>) {
  props.value = { volume: volume([]), opening: null, error: null, ...next };
  app = createApp({ render: () => h(ShadowCopyList, { ...props.value, onOpen: opened }) });
  app.mount(host);
  await nextTick();
}

function rows(): HTMLLIElement[] {
  return [...host.querySelectorAll<HTMLLIElement>('.copy-row')];
}

beforeEach(() => {
  vi.clearAllMocks();
  host = document.createElement('div');
  document.body.append(host);
});
afterEach(() => {
  app.unmount();
  host.remove();
});

describe('shadow copy list', () => {
  it('lists shadow copies in creation order with their UTC time', async () => {
    await mount({ volume: volume([copy(0), copy(1, { client_accessible: true })]) });
    const [first, second] = rows();
    expect(first.querySelector('.copy-index')?.textContent).toBe('#1');
    expect(first.querySelector('.copy-time')?.textContent).toBe('2021-05-01 17:40:03 UTC');
    expect(second.textContent).toContain('客户端可访问');
  });

  it('hands the chosen copy to the page and marks the one being rebuilt', async () => {
    const copies = [copy(0), copy(1, { open_image_id: 'session' })];
    await mount({ volume: volume(copies) });
    expect(rows()[1].querySelector('button')?.textContent).toContain('切换到');
    rows()[0].querySelector('button')!.click();
    expect(opened).toHaveBeenCalledWith(copies[0]);

    app.unmount();
    await mount({ volume: volume(copies), opening: '2:store-0' });
    expect(rows()[0].querySelector('button')?.textContent).toContain('正在重建');
    expect(rows().every(row => row.querySelector('button')!.disabled)).toBe(true);
  });

  it('keeps unavailable copies closed and reports metadata and open failures', async () => {
    await mount({ volume: volume([copy(0, { unavailable_reason: 'store 0 has no store-information pointer' })]), error: 'store 0 metadata block at 0x4000 is corrupt' });
    const button = rows()[0].querySelector('button')!;
    expect(button.disabled).toBe(true);
    expect(button.title).toContain('no store-information pointer');
    expect(host.querySelector('[role=alert]')?.textContent).toContain('is corrupt');

    app.unmount();
    await mount({ volume: volume([], 'shadow copy evidence contains unreadable sectors') });
    expect(rows()).toHaveLength(0);
    expect(host.querySelector('[role=alert]')?.textContent).toContain('unreadable sectors');
  });
});
