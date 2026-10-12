import { beforeEach, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../../../lib/ipc/client', () => ({ invokeCmd: invoke }));
import { createPreviewRequest } from './previewRequest';
beforeEach(() => { invoke.mockReset(); invoke.mockResolvedValue(undefined); });
it('uses distinct IDs for viewers and releases a materialization returned after cancellation', async () => {
  let resolve!: (path: string) => void;
  invoke.mockImplementation(command => command === 'fs_media_temp'
    ? new Promise(done => { resolve = done; }) : Promise.resolve());
  const first = createPreviewRequest(); const second = createPreviewRequest();
  expect(first.taskId).not.toBe(second.taskId);
  const result = first.materialize({ imageId: 'image', nodeId: 'node' });
  first.cancel();
  resolve('temporary.pdf');
  expect(await result).toBeNull();
  expect(invoke).toHaveBeenCalledWith('fs_media_release', { taskId: first.taskId });
  expect(invoke).not.toHaveBeenCalledWith('task_cancel', { taskId: second.taskId });
});
it('keeps a successful lease until the viewer closes', async () => {
  invoke.mockResolvedValue('temporary.mp4');
  const request = createPreviewRequest();
  expect(await request.materialize({ imageId: 'image', nodeId: 'node' })).toBe('temporary.mp4');
  expect(invoke).not.toHaveBeenCalledWith('fs_media_release', expect.anything());
  request.cancel();
  expect(invoke).toHaveBeenCalledWith('fs_media_release', { taskId: request.taskId });
});
