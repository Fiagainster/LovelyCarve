import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RaidStack } from '../../../lib/ipc/raid';
const ipc = vi.hoisted(() => ({ detect: vi.fn(), confirm: vi.fn(), dismiss: vi.fn() }));
vi.mock('../../../lib/ipc/raid', () => ({ raidAutoDetect: ipc.detect }));
vi.mock('../../../lib/ipc/image', () => ({ imageInfoById: vi.fn() }));
vi.mock('../../../composables/useConfirm', () => ({ confirm: ipc.confirm }));
vi.mock('../../../composables/useToast', () => ({ notifyError: vi.fn(), pushPersistentToast: () => ({ dismiss: ipc.dismiss }) }));
import { disposeRaidStack, useRaidStack } from './useRaidStack';
const stack: RaidStack = { images: [], arrays: [], volume_groups: [], filesystems: [], diagnostics: [] };
beforeEach(() => { disposeRaidStack(); vi.resetAllMocks(); });

describe('RAID case lifetime', () => {
  it('clears the old detection cache when the source case closes', async () => {
    ipc.detect.mockResolvedValue(stack);
    const state = useRaidStack();
    await state.detect({ promptAssembled: false });
    expect(state.stack.value).toEqual(stack);
    disposeRaidStack();
    expect([state.stack.value, state.loading.value, state.error.value]).toEqual([null, false, null]);
  });
  it('discards detection that completes after the case closes', async () => {
    let finish!: (value: RaidStack) => void;
    ipc.detect.mockReturnValue(new Promise<RaidStack>(resolve => { finish = resolve; }));
    const state = useRaidStack();
    const result = state.detect();
    disposeRaidStack();
    finish(stack);
    expect(await result).toBeNull();
    expect(state.stack.value).toBeNull();
  });
});
