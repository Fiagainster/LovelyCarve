// @vitest-environment happy-dom
import { createApp, defineComponent, nextTick, ref, type App } from 'vue';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ShadowCopyInfo, ShadowVolumeInfo } from '../../../lib/ipc/shadow';

const rpc = vi.hoisted(() => ({ list: vi.fn(), open: vi.fn() }));
vi.mock('../../../lib/ipc/shadow', () => ({ shadowCopyList: rpc.list, shadowCopyOpen: rpc.open }));
import { useShadowCopies } from './useShadowCopies';

function copy(index: number): ShadowCopyInfo {
  return {
    index, store_id: `store-${index}`, shadow_copy_id: null, shadow_copy_set_id: null, creation_filetime: 0,
    created_unix_ms: null, volume_size: 1, snapshot_context: null, attributes: null, persistent: true,
    client_accessible: false, operating_machine: null, service_machine: null, unavailable_reason: null, open_image_id: null,
  };
}

function volume(partition_id: number, shadow_copies: ShadowCopyInfo[]): ShadowVolumeInfo {
  return { partition_id, volume_size: 1, shadow_copies, error: null };
}

let app: App;
let state: ReturnType<typeof useShadowCopies>;
const imageId = ref<string | null>('source');

async function settle() {
  for (let i = 0; i < 4; i += 1) await nextTick();
}

beforeEach(async () => {
  vi.clearAllMocks();
  imageId.value = 'source';
  app = createApp(defineComponent({
    setup() {
      state = useShadowCopies(() => imageId.value);
      return () => null;
    },
  }));
});
afterEach(() => app.unmount());

async function mount() {
  app.mount(document.createElement('div'));
  await settle();
}

describe('useShadowCopies', () => {
  it('lists the copies of the current image and finds a volume by partition', async () => {
    rpc.list.mockResolvedValue([volume(2, [copy(0)])]);
    await mount();
    expect(rpc.list).toHaveBeenCalledWith({ imageId: 'source' });
    expect(state.volumeOf(2)?.shadow_copies).toHaveLength(1);
    expect(state.volumeOf(3)).toBeUndefined();
  });

  it('skips evidence without VSS and retries a failed listing', async () => {
    imageId.value = null;
    rpc.list.mockRejectedValueOnce(new Error('image not loaded')).mockResolvedValueOnce([volume(2, [])]);
    await mount();
    expect(rpc.list).not.toHaveBeenCalled();

    imageId.value = 'source';
    await settle();
    expect(state.loadError.value).toContain('image not loaded');
    await state.load();
    expect(state.loadError.value).toBe('');
    expect(state.volumes.value).toHaveLength(1);
  });

  it('opens a copy, remembers its session and keeps a failure with its volume', async () => {
    const target = volume(2, [copy(0), copy(1)]);
    rpc.list.mockResolvedValue([target]);
    rpc.open
      .mockResolvedValueOnce({ id: 'snapshot', path: 'vss://source/2/store-1/分区 2 卷影副本 2' })
      .mockRejectedValueOnce(new Error('store 0 metadata block at 0x4000 is corrupt'));
    await mount();
    const listed = state.volumeOf(2)!;

    await expect(state.open(listed, listed.shadow_copies[1])).resolves.toBe('vss://source/2/store-1/分区 2 卷影副本 2');
    expect(rpc.open).toHaveBeenCalledWith(expect.objectContaining({ imageId: 'source', partitionId: 2, index: 1 }));
    expect(state.volumeOf(2)!.shadow_copies[1].open_image_id).toBe('snapshot');

    await expect(state.open(listed, listed.shadow_copies[0])).resolves.toBeNull();
    expect(state.openError.value).toEqual({ partitionId: 2, message: 'Error: store 0 metadata block at 0x4000 is corrupt' });
    expect(state.opening.value).toBeNull();
  });

  it('drops an answer that arrives after the page moved to another image', async () => {
    let resolveFirst!: (value: ShadowVolumeInfo[]) => void;
    rpc.list
      .mockReturnValueOnce(new Promise(resolve => { resolveFirst = resolve; }))
      .mockResolvedValueOnce([volume(5, [])]);
    await mount();
    imageId.value = 'other';
    await settle();
    resolveFirst([volume(2, [copy(0)])]);
    await settle();
    expect(state.volumes.value.map(listed => listed.partition_id)).toEqual([5]);
  });
});
