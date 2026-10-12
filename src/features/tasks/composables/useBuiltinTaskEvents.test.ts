import { describe, expect, it, vi } from 'vitest';
import { createBuiltinTaskEvents } from './useBuiltinTaskEvents';
describe('built-in task event owner', () => {
  it('retries only failed listeners and detaches successful listeners once', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {}); const stop = vi.fn(); const recovered = vi.fn();
    const first = vi.fn(async () => stop); const second = vi.fn().mockRejectedValueOnce(new Error('bridge')).mockResolvedValue(recovered);
    const owner = createBuiltinTaskEvents([first, second]); await owner.ensure(); await owner.ensure();
    expect(first).toHaveBeenCalledOnce(); expect(second).toHaveBeenCalledTimes(2);
    owner.dispose(); owner.dispose(); expect(stop).toHaveBeenCalledOnce(); expect(recovered).toHaveBeenCalledOnce(); warn.mockRestore();
  });
  it('releases an attachment resolving after disposal', async () => {
    let resolve!: (stop: () => void) => void; const stop = vi.fn();
    const owner = createBuiltinTaskEvents([() => new Promise(yes => { resolve = yes; })]);
    const attaching = owner.ensure(); owner.dispose(); resolve(stop); await attaching; expect(stop).toHaveBeenCalledOnce();
  });
});
