import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useResultNavigation } from './useResultNavigation';
import { type FileListNavigator } from '../../features/files/model/navigation';

let caseKey: string | null;
let imageId: string | null;
let panel: FileListNavigator | null;
let filesActive: boolean;
const openFile = vi.fn();
const openSource = vi.fn();
const openImage = vi.fn();
const onError = vi.fn();
function navigation() {
  return useResultNavigation({ caseKey: () => caseKey, imageId: () => imageId, openImage,
    showFiles: () => { filesActive = true; }, isFilesActive: () => filesActive, panel: () => panel, onError });
}
const target = { imageId: 'disk-b', nodeId: 'part:0:42', offset: 4_000_000, length: 8 };
beforeEach(() => {
  vi.useFakeTimers(); vi.resetAllMocks();
  caseKey = 'case-a'; imageId = 'disk-a'; filesActive = false;
  panel = { imageId: 'disk-a', openFileByNodeId: openFile, openFileBySource: openSource };
  openImage.mockImplementation(async id => { imageId = id; return true; });
});
afterEach(() => vi.useRealTimers());

describe('result navigation', () => {
  it('activates the source evidence and waits for its panel, preserving the exact byte target', async () => {
    const hub = navigation();
    const operation = hub.openLocation(target);
    await vi.advanceTimersByTimeAsync(50);
    expect(openImage).toHaveBeenCalledWith('disk-b');
    expect(openFile).not.toHaveBeenCalled();
    panel = { imageId: 'disk-b', openFileByNodeId: openFile, openFileBySource: openSource };
    await vi.advanceTimersByTimeAsync(50);
    await operation;
    expect(openFile).toHaveBeenCalledExactlyOnceWith('part:0:42', 4_000_000, 8);
    expect(hub.locating.value).toBe(false);
  });

  it('locates timeline files without forcing a byte offset', async () => {
    await navigation().openLocation({ imageId: 'disk-a', nodeId: 'file' });
    expect(openImage).not.toHaveBeenCalled();
    expect(openFile).toHaveBeenCalledWith('file', undefined, undefined);
  });

  it('routes plugin paths through their selected volume without forcing a byte preview', async () => {
    await navigation().openLocation({ imageId: 'disk-a', rootId: 'host-docker:container:', path: 'etc/passwd' });
    expect(openSource).toHaveBeenCalledExactlyOnceWith('host-docker:container:', 'etc/passwd');
    expect(openFile).not.toHaveBeenCalled();
  });

  it.each(['case', 'image', 'panel'])('abandons a pending jump when the user switches %s', async kind => {
    const operation = navigation().openLocation(target);
    await vi.advanceTimersByTimeAsync(50);
    if (kind === 'case') caseKey = 'case-b';
    if (kind === 'image') imageId = 'disk-c';
    if (kind === 'panel') filesActive = false;
    panel = { imageId: 'disk-b', openFileByNodeId: openFile, openFileBySource: openSource };
    await vi.advanceTimersByTimeAsync(50);
    await operation;
    expect(openFile).not.toHaveBeenCalled();
    expect(onError).not.toHaveBeenCalled();
  });

  it('reports an unavailable evidence instead of locating in the active image', async () => {
    openImage.mockResolvedValue(false);
    await navigation().openLocation(target);
    expect(openFile).not.toHaveBeenCalled();
    expect(onError.mock.calls[0][0]).toContain('无法打开来源检材');
  });

  it('reports panel timeouts and unlocks the controls', async () => {
    const hub = navigation();
    const operation = hub.openLocation(target);
    await vi.advanceTimersByTimeAsync(5100);
    await operation;
    expect(onError.mock.calls[0][0]).toContain('文件列表未能及时加载');
    expect(hub.locating.value).toBe(false);
  });

  it('prevents duplicate requests and forwards source lookup failures', async () => {
    openFile.mockRejectedValue(new Error('文件不存在'));
    const hub = navigation();
    await Promise.all([hub.openLocation({ ...target, imageId: 'disk-a' }), hub.openLocation(target)]);
    expect(openFile).toHaveBeenCalledTimes(1);
    expect(onError.mock.calls[0][0]).toContain('文件不存在');
  });

  it('rejects offsets that cannot be represented exactly', async () => {
    await navigation().openLocation({ ...target, offset: Number.MAX_SAFE_INTEGER + 1 });
    expect(openImage).not.toHaveBeenCalled();
    expect(onError.mock.calls[0][0]).toContain('命中偏移无效');
  });
});
