import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { CaseInfo } from '../../../lib/ipc/cases';
import { useCaseSession, disposeCaseSession, registerCaseWorkspaceTeardown } from './useCaseSession';
const ipc = vi.hoisted(() => ({ active: vi.fn(), open: vi.fn(), close: vi.fn(), list: vi.fn(), snapshots: vi.fn(), listen: vi.fn() }));
vi.mock('../../../lib/ipc/cases', () => ({ caseActive: ipc.active, caseOpen: ipc.open, caseClose: ipc.close, caseList: ipc.list, snapshotList: ipc.snapshots, SNAPSHOT_PROGRESS_EVENT: 'snapshot-build-progress' }));
vi.mock('../../../lib/ipc/events', () => ({ onEvent: ipc.listen }));
vi.mock('./useRecentCases', () => ({ useRecentCases: () => ({ recordCase: vi.fn(), removeCase: vi.fn() }) }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }));
function deferred<T>() { let resolve!: (value: T) => void; return { promise: new Promise<T>(yes => { resolve = yes; }), resolve: (value: T) => resolve(value) }; }
const info = (dir: string) => ({ dir, name: dir, evidence: [] } as unknown as CaseInfo);
async function settle() { for (let index = 0; index < 8; index++) await Promise.resolve(); }
beforeEach(() => { disposeCaseSession(); vi.clearAllMocks(); ipc.listen.mockResolvedValue(vi.fn()); ipc.list.mockResolvedValue([]); ipc.snapshots.mockResolvedValue([]); });
afterEach(disposeCaseSession);
describe('case workspace lifetime', () => {
  it('keeps previous cases when registry loading fails and clears the error after retry', async () => {
    const session = useCaseSession();
    ipc.list.mockResolvedValueOnce([{ dir: 'known' }]); await session.refreshCases();
    ipc.list.mockRejectedValueOnce(new Error('登记簿不可读')); await session.refreshCases();
    expect(session.cases.value).toEqual([{ dir: 'known' }]);
    expect(session.registryError.value).toContain('登记簿不可读');
    expect(session.registryLoading.value).toBe(false);
    await session.refreshCases(); expect(session.registryError.value).toBe('');
  });

  it('does not publish late registry errors into a replacement session', async () => {
    const session = useCaseSession();
    let reject!: (reason: Error) => void;
    ipc.list.mockReturnValueOnce(new Promise((_resolve, no) => { reject = no; }));
    const loading = session.refreshCases(); expect(session.registryLoading.value).toBe(true);
    disposeCaseSession(); const replacement = useCaseSession();
    reject(new Error('old')); await loading;
    expect(replacement.registryError.value).toBe('');
    expect(replacement.registryLoading.value).toBe(false);
  });
  it('waits for pending writes before opening or closing the backend case', async () => {
    const session = useCaseSession(); const flush = deferred<void>();
    registerCaseWorkspaceTeardown(() => flush.promise);
    ipc.open.mockResolvedValue(info('new'));
    const opening = session.openCase('new');
    await settle(); expect(ipc.open).not.toHaveBeenCalled();
    flush.resolve(); await opening;
    const closingFlush = deferred<void>(); registerCaseWorkspaceTeardown(() => closingFlush.promise);
    const closing = session.closeCase(); await settle(); expect(ipc.close).not.toHaveBeenCalled();
    closingFlush.resolve(); await closing; expect(ipc.close).toHaveBeenCalledOnce();
  });
  it('rejects an old active-case read after a case operation', async () => {
    const session = useCaseSession(); const old = deferred<CaseInfo>(); ipc.active.mockReturnValue(old.promise);
    const reading = session.refreshActive(); ipc.open.mockResolvedValue(info('new')); await session.openCase('new');
    old.resolve(info('old')); await reading; expect(session.activeCase.value?.dir).toBe('new');
  });
  it('releases a late listener and does not attach its callback to a replacement owner', async () => {
    const late = deferred<() => void>(); const stop = vi.fn(); ipc.listen.mockReturnValueOnce(late.promise);
    useCaseSession(); const callback = ipc.listen.mock.calls[0][1]; disposeCaseSession();
    const replacement = useCaseSession(); late.resolve(stop); await settle();
    expect(stop).toHaveBeenCalledOnce(); callback({ payload: { image_id: 'old', volume_key: 'root', phase: 'walking' } });
    expect(replacement.buildProgress.value.size).toBe(0);
  });
  it('retries listener initialization after failure and shares a successful subscription', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    ipc.listen.mockRejectedValueOnce(new Error('bridge')); useCaseSession(); await settle();
    useCaseSession(); useCaseSession(); await settle(); expect(ipc.listen).toHaveBeenCalledTimes(2);
    vi.restoreAllMocks();
  });
  it('ignores a late open after disposal and prevents overlapping case mutations', async () => {
    const session = useCaseSession(); const late = deferred<CaseInfo>(); ipc.open.mockReturnValue(late.promise);
    const opening = session.openCase('old'); await settle(); await expect(session.openCase('other')).rejects.toThrow('案件操作正在进行');
    disposeCaseSession(); late.resolve(info('old')); await opening; expect(session.activeCase.value).toBeNull();
  });
});
