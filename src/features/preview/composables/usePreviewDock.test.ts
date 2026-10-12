import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FilePreviewReadingState } from '../../../lib/models/filePreviewReadingState';

const mocks = vi.hoisted(() => ({
  accept: undefined as ((id: string, context?: FilePreviewReadingState) => Promise<boolean>) | undefined,
  dispose: undefined as (() => void) | undefined,
  snapshot: undefined as ((id: string) => Promise<void>) | undefined,
  cleanup: vi.fn(), selectionCleanup: vi.fn(), publish: vi.fn(), focus: vi.fn(), open: vi.fn(), notify: vi.fn(),
}));
vi.mock('vue', async importOriginal => ({
  ...await importOriginal<typeof import('vue')>(),
  onBeforeUnmount: (callback: () => void) => { mocks.dispose = callback; },
}));
vi.mock('../../../lib/ipc/previewDock', () => ({
  onPreviewDockRequest: vi.fn(async (_owner: string, accept: (id: string) => Promise<boolean>) => {
    mocks.accept = accept;
    return mocks.cleanup;
  }),
  onPreviewSelectionRequest: vi.fn(async (_owner: string, respond: (id: string) => Promise<void>) => {
    mocks.snapshot = respond;
    return mocks.selectionCleanup;
  }),
  publishPreviewSelection: mocks.publish,
}));
vi.mock('../../../lib/ipc/windows', () => ({ openFilePreviewWindow: mocks.open, focusFilePreviewWindow: mocks.focus }));
vi.mock('../../../composables/useToast', () => ({ notifyError: mocks.notify }));
import { usePreviewDock } from './usePreviewDock';
import { onPreviewSelectionRequest } from '../../../lib/ipc/previewDock';

const args = { imageId: 'image-a', nodeId: 'file', node: { id: 'file', path: '/original/file' }, fullPath: '/original/file', title: 'file' };
beforeEach(() => {
  vi.clearAllMocks();
  mocks.open.mockResolvedValue(undefined);
  mocks.publish.mockResolvedValue(undefined);
  mocks.focus.mockResolvedValue(true);
});

describe('preview ownership', () => {
  it('restores the detached reading position and rejects context for an older file', async () => {
    const restore = vi.fn();
    const dock = usePreviewDock({ imageId: () => 'image-a', restore });
    const readingState: FilePreviewReadingState = { imageId: 'image-a', nodeId: 'file', tab: 'text', searchQuery: 'needle', encoding: 'utf-8', page: 2, pageSize: 2048, currentMatch: 1, scrollTop: 220, scrollLeft: 40, codeScrollTop: 0, codeScrollLeft: 0 };
    await dock.detach({ ...args, readingState });
    const previewId = mocks.open.mock.calls[0][0].previewId;
    expect(mocks.open.mock.calls[0][0].readingState).toEqual(readingState);
    expect(await mocks.accept?.(previewId, { ...readingState, nodeId: 'older-file' })).toBe(false);
    expect(restore).not.toHaveBeenCalled();
    const latest = { ...readingState, tab: 'hex' as const, page: 3 };
    expect(await mocks.accept?.(previewId, latest)).toBe(true);
    expect(restore).toHaveBeenCalledExactlyOnceWith(args.node, undefined, latest);
  });
  it('retains the search byte target when detached preview docks back', async () => {
    const restore = vi.fn();
    const focusTarget = { offset: 4_000_000, length: 8 };
    const dock = usePreviewDock({ imageId: () => 'image-a', restore });
    await dock.detach({ ...args, focusTarget });
    const [launch] = mocks.open.mock.calls[0];
    expect(launch.focusTarget).toEqual(focusTarget);
    expect(await mocks.accept?.(launch.previewId)).toBe(true);
    expect(restore).toHaveBeenCalledWith(args.node, focusTarget);
  });

  it('restores the captured path after directory navigation and releases closed windows', async () => {
    const restore = vi.fn();
    const dock = usePreviewDock({ imageId: () => 'image-a', restore });
    expect(await dock.detach(args)).toBe(true);
    const [launch, onClosed] = mocks.open.mock.calls[0];
    expect(await mocks.accept?.(launch.previewId)).toBe(true);
    expect(restore).toHaveBeenCalledExactlyOnceWith(args.node);
    onClosed();
    expect(await mocks.accept?.(launch.previewId)).toBe(false);
  });

  it('rejects a cached list when the shell has switched to another evidence', async () => {
    const restore = vi.fn();
    const dock = usePreviewDock({ imageId: () => 'image-a', activate: () => false, restore });
    await dock.detach(args);
    expect(await mocks.accept?.(mocks.open.mock.calls[0][0].previewId)).toBe(false);
    expect(restore).not.toHaveBeenCalled();
  });

  it('keeps the dock available when opening the native window fails', async () => {
    mocks.open.mockRejectedValueOnce(new Error('window unavailable'));
    const dock = usePreviewDock({ imageId: () => 'image-a', restore: vi.fn() });
    expect(await dock.detach(args)).toBe(false);
    expect(dock.opening.value).toBe(false);
    expect(mocks.notify).toHaveBeenCalledWith('打开独立预览失败', 'Error: window unavailable');
    expect(await mocks.accept?.(mocks.open.mock.calls[0][0].previewId)).toBe(false);
  });

  it('removes subscriptions and rejects restoration after unmount', async () => {
    const dock = usePreviewDock({ imageId: () => 'image-a', restore: vi.fn() });
    await dock.detach(args);
    mocks.dispose?.();
    expect(mocks.cleanup).toHaveBeenCalledOnce();
    expect(mocks.selectionCleanup).toHaveBeenCalledOnce();
    expect(await mocks.accept?.(mocks.open.mock.calls[0][0].previewId)).toBe(false);
    expect(await dock.detach(args)).toBe(false);
  });

  it('follows the latest file, reuses its window and docks the latest byte target', async () => {
    const restore = vi.fn();
    const dock = usePreviewDock({ imageId: () => 'image-a', restore });
    await dock.detach(args);
    const latest = { ...args, nodeId: 'second', node: { id: 'second', path: '/other/second' }, fullPath: '/other/second', title: 'second', focusTarget: { offset: 512, length: 8 } };
    await dock.sync(latest);
    expect(dock.detached.value).toBe(true);
    const selection = mocks.publish.mock.lastCall![0];
    expect(selection.args).toEqual(latest);
    await dock.detach(latest);
    expect(mocks.open).toHaveBeenCalledOnce();
    expect(mocks.focus).toHaveBeenCalledExactlyOnceWith(selection.previewId);
    expect(await mocks.accept?.(selection.previewId)).toBe(true);
    expect(restore).toHaveBeenCalledExactlyOnceWith(latest.node, latest.focusTarget);
    expect(dock.detached.value).toBe(false);
  });

  it('retains changes made while the native window is still opening', async () => {
    let opened!: () => void;
    mocks.open.mockImplementationOnce(() => new Promise<void>(resolve => { opened = resolve; }));
    const dock = usePreviewDock({ imageId: () => 'image-a', restore: vi.fn() });
    const opening = dock.detach(args);
    await vi.waitFor(() => expect(mocks.open).toHaveBeenCalledOnce());
    expect(dock.detached.value).toBe(false);
    const latest = { ...args, nodeId: 'latest', node: { id: 'latest', path: '/latest' }, fullPath: '/latest' };
    await dock.sync(latest);
    opened();
    expect(await opening).toBe(true);
    expect(mocks.publish.mock.lastCall![0].args).toEqual(latest);
    mocks.publish.mockClear();
    await mocks.snapshot?.(mocks.open.mock.calls[0][0].previewId);
    expect(mocks.publish.mock.lastCall![0].args).toEqual(latest);
  });

  it('opens the latest file when listener registration is still pending', async () => {
    let registered!: (cleanup: () => void) => void;
    vi.mocked(onPreviewSelectionRequest).mockImplementationOnce(() => new Promise(resolve => { registered = resolve; }));
    const dock = usePreviewDock({ imageId: () => 'image-a', restore: vi.fn() });
    const opening = dock.detach(args);
    const latest = { ...args, nodeId: 'latest', node: { id: 'latest', path: '/latest' }, fullPath: '/latest' };
    await dock.sync(latest);
    await vi.waitFor(() => expect(registered).toBeTypeOf('function'));
    registered(mocks.selectionCleanup);
    expect(await opening).toBe(true);
    expect(mocks.open.mock.calls[0][0]).toMatchObject(latest);
    expect(mocks.publish.mock.lastCall![0].args).toEqual(latest);
  });

  it('releases detached mode on close and ignores a closed window snapshot request', async () => {
    const closed = vi.fn();
    const dock = usePreviewDock({ imageId: () => 'image-a', closed, restore: vi.fn() });
    await dock.detach(args);
    const [launch, onClosed] = mocks.open.mock.calls[0];
    onClosed();
    expect(dock.detached.value).toBe(false);
    expect(closed).toHaveBeenCalledOnce();
    mocks.publish.mockClear();
    await mocks.snapshot?.(launch.previewId);
    await dock.sync(args);
    expect(mocks.publish).not.toHaveBeenCalled();
  });

  it('follows an explicitly selected file in another evidence without accepting stale docking', async () => {
    let imageId = 'image-a';
    const restore = vi.fn();
    const dock = usePreviewDock({ imageId: () => imageId, restore });
    await dock.detach(args);
    const previewId = mocks.open.mock.calls[0][0].previewId;
    imageId = 'image-b';
    expect(await mocks.accept?.(previewId)).toBe(false);
    await dock.sync({ ...args, imageId });
    expect(await mocks.accept?.(previewId)).toBe(true);
    expect(restore).toHaveBeenCalledWith(args.node);
  });

  it('keeps the detached destination when publishing fails after native creation', async () => {
    mocks.publish.mockRejectedValueOnce(new Error('event unavailable'));
    const dock = usePreviewDock({ imageId: () => 'image-a', restore: vi.fn() });
    expect(await dock.detach(args)).toBe(true);
    expect(dock.detached.value).toBe(true);
    expect(mocks.notify).toHaveBeenCalledWith('同步独立预览失败', 'Error: event unavailable');
  });
});
