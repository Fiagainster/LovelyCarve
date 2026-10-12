// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, h, nextTick, ref, type App } from 'vue';
import CarvePanel from './CarvePanel.vue';
const ipc = vi.hoisted(() => ({ list: vi.fn(), remove: vi.fn(), confirm: vi.fn(), progress: vi.fn(), start: vi.fn(), status: vi.fn(), page: vi.fn(), pause: vi.fn(), resume: vi.fn(), recover: vi.fn(), open: vi.fn() }));
vi.mock('../../tasks/composables/useBuiltinTasks', () => ({ trackedCarveStart: ipc.start }));
vi.mock('../../../composables/useConfirm', () => ({ confirm: ipc.confirm }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: ipc.open }));
vi.mock('../../../composables/useToast', () => ({ notifyError: vi.fn(), notifyInfo: vi.fn(), notifySuccess: vi.fn() }));
vi.mock('../../../lib/ipc/taskRuntime', () => ({ onTaskProgress: vi.fn(async () => () => {}), taskCancel: vi.fn() }));
vi.mock('../../../lib/ipc/carve', () => ({
  carveFormats: vi.fn(async () => [
    { id: 'jpg', label: 'JPEG', category: 'image', extension: 'jpg' },
    { id: 'png', label: 'PNG', category: 'image', extension: 'png' },
    { id: 'pdf', label: 'PDF', category: 'document', extension: 'pdf' },
  ]), carveScanList: ipc.list, carveScanRemove: ipc.remove, carveStart: ipc.start, carveScanStatus: ipc.status,
  carveResultsPage: ipc.page, carveScanPause: ipc.pause, carveScanResume: ipc.resume,
  carveRecover: ipc.recover, carvePreview: vi.fn(), onCarveScanProgress: ipc.progress,
}));
const scan = { scan_id: 'scan-a', task_id: 'task-a', image_id: 'disk-a', created_unix_ms: 100, scope_label: '整个镜像', status: 'completed', artifacts_found: 2, signatures_found: 2, bytes_scanned: 1024, bytes_total: 1024 };
const artifacts = ['one.pdf', 'two.pdf'].map((name, index) => ({ id: `file-${index}`, name, format_id: 'pdf', extension: 'pdf', category: 'document', offset: index * 512, length: 512, confidence: 90, region: '分区 0', read_fault: false }));
let app: App | null = null;
const imageId = ref('disk-a');
async function settle() { for (let i = 0; i < 15; i++) await nextTick(); }
async function mount() { const host = document.createElement('div'); document.body.append(host); app = createApp({ render: () => h(CarvePanel, { imageId: imageId.value, imageInfo: null }) }); app.mount(host); await settle(); }
function button(text: string) { const element = [...document.querySelectorAll('button')].find(item => item.textContent?.trim() === text); if (!element) throw new Error(`Missing ${text}`); return element; }
beforeEach(() => { vi.resetAllMocks(); imageId.value = 'disk-a'; ipc.list.mockResolvedValue([]); ipc.progress.mockResolvedValue(() => {}); ipc.confirm.mockResolvedValue(true); ipc.status.mockResolvedValue(scan); ipc.start.mockResolvedValue({ scan_id: 'scan-a' }); ipc.page.mockResolvedValue({ items: artifacts, total: 2, next_cursor: null, resume_cursor: null }); ipc.open.mockResolvedValue('D:/recovered'); ipc.recover.mockResolvedValue({ recovered: 1, failed: 0, bytes_written: 512, report_path: 'report.xml' }); });
afterEach(() => { app?.unmount(); app = null; document.body.replaceChildren(); });
describe('carving search-style workspace', () => {
  it('selects formats by category and submits the same scan options with collapsed advanced settings', async () => {
    await mount();
    expect(document.querySelector('[role=dialog]')).toBeNull();
    button('新建扫描').click(); await settle();
    expect(document.querySelector('[role=dialog]')).not.toBeNull();
    expect(document.querySelector('details')?.open).toBe(false);
    expect(button('指定分区').disabled).toBe(true);
    button('清空').click(); await settle();
    expect(button('开始扫描').disabled).toBe(true);
    (document.querySelector('.group-label input') as HTMLInputElement).click(); await settle();
    expect(button('开始扫描').disabled).toBe(false);
    (document.querySelectorAll('.format-chip input')[0] as HTMLInputElement).click(); await settle();
    expect((document.querySelector('.group-label input') as HTMLInputElement).indeterminate).toBe(true);
    ipc.list.mockResolvedValue([scan]); button('开始扫描').click(); await settle();
    expect(ipc.start).toHaveBeenCalledWith({ imageId: 'disk-a', options: { format_ids: ['png'], scope: 'whole', partition_id: null, max_file_size: 10 * 1024 ** 3, max_results: 20000 } });
  });
  it('opens and closes details on demand, filters by filename, and recovers only checked results', async () => {
    ipc.list.mockResolvedValue([scan]); await mount();
    expect(document.querySelector('.detail-dock')).toBeNull();
    (document.querySelector('tbody tr') as HTMLElement).click(); await settle();
    expect(document.querySelector('.detail-dock')?.textContent).toContain('one.pdf');
    (document.querySelector('[aria-label="关闭文件详情"]') as HTMLButtonElement).click(); await settle();
    expect(document.querySelector('.detail-dock')).toBeNull();
    const search = document.querySelector('input[type="search"]') as HTMLInputElement;
    search.value = 'two.pdf'; search.dispatchEvent(new Event('input')); await settle();
    search.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await settle();
    expect(document.querySelectorAll('tbody tr')).toHaveLength(1);
    (document.querySelector('tbody input') as HTMLInputElement).click(); await settle();
    button('恢复所选（1）').click(); await settle();
    expect(ipc.recover).toHaveBeenCalledWith({ scanId: 'scan-a', options: { artifact_ids: ['file-1'], destination: 'D:/recovered', task_id: expect.any(String) } });
  });
  it('keeps pause and resume available without exposing configuration during scanning', async () => {
    ipc.list.mockResolvedValue([{ ...scan, status: 'running' }]); ipc.pause.mockResolvedValue({ ...scan, status: 'paused' }); ipc.resume.mockResolvedValue({ ...scan, status: 'running' }); await mount();
    expect(document.querySelector('[role=dialog]')).toBeNull();
    button('暂停').click(); await settle(); expect(ipc.pause).toHaveBeenCalledWith({ scanId: 'scan-a' });
    button('继续').click(); await settle(); expect(ipc.resume).toHaveBeenCalledWith({ scanId: 'scan-a' });
  });
  it('dismisses scan settings without replacing the current results', async () => {
    ipc.list.mockResolvedValue([scan]); await mount();
    button('新建扫描').click(); await settle();
    expect(document.querySelector('[role=dialog]')).not.toBeNull();
    button('取消').click(); await settle();
    await vi.waitFor(() => expect(document.querySelector('[role=dialog]')).toBeNull());
    expect(document.querySelectorAll('tbody tr')).toHaveLength(2);
    expect(ipc.start).not.toHaveBeenCalled();
  });

});


describe('carving history', () => {
  it('opens retained records and switches the selected scan without accepting stale results', async () => {
    const older = { ...scan, scan_id: 'scan-b', scope_label: '分区 1', created_unix_ms: 50 };
    ipc.list.mockResolvedValue([scan, older]);
    let resolve!: (value: unknown) => void;
    ipc.page.mockReturnValueOnce(new Promise(finish => { resolve = finish; }));
    await mount();
    expect((document.querySelector('.task-pane') as HTMLElement).style.display).not.toBe('none');
    ipc.page.mockResolvedValue({ items: [{ ...artifacts[1], name: 'older.pdf' }], total: 1, next_cursor: null });
    (document.querySelectorAll('.task-open')[1] as HTMLButtonElement).click(); await settle();
    expect(ipc.page).toHaveBeenLastCalledWith({ scanId: 'scan-b', cursor: null, limit: 200 });
    expect(document.querySelector('tbody')?.textContent).toContain('older.pdf');
    resolve({ items: artifacts, total: 2, next_cursor: null }); await settle();
    expect(document.querySelector('tbody')?.textContent).not.toContain('one.pdf');
    expect(document.querySelector('tbody')?.textContent).toContain('older.pdf');
    ipc.progress.mock.calls[0][0]({ payload: { ...scan, status: 'running', artifacts_found: 3 } }); await settle();
    expect(document.querySelector('.result-summary')?.textContent).toContain('分区 1');
  });

  it('deletes a finished record and selects the next, while active records cannot be deleted', async () => {
    const active = { ...scan, scan_id: 'scan-b', scope_label: '分区 1', status: 'running' };
    ipc.list.mockResolvedValue([scan, active]); await mount();
    expect((document.querySelectorAll('.task-delete')[1] as HTMLButtonElement).disabled).toBe(true);
    ipc.list.mockResolvedValue([active]);
    (document.querySelector('.task-delete') as HTMLButtonElement).click(); await settle();
    expect(ipc.remove).toHaveBeenCalledWith({ imageId: 'disk-a', scanId: 'scan-a' });
    expect(document.querySelectorAll('.task-open')).toHaveLength(1);
    expect(document.querySelector('.result-summary')?.textContent).toContain('分区 1');
  });

  it('automatically opens the first scan arriving through progress events', async () => {
    await mount();
    expect((document.querySelector('.task-pane') as HTMLElement).style.display).toBe('none');
    ipc.progress.mock.calls[0][0]({ payload: { ...scan, status: 'running' } }); await settle();
    expect((document.querySelector('.task-pane') as HTMLElement).style.display).not.toBe('none');
    expect(document.querySelectorAll('.task-open')).toHaveLength(1);
    expect(document.querySelector('.result-summary')?.textContent).toContain('整个镜像');
  });
});


it('focuses scan settings so Escape dismisses them and restores focus', async () => {
  await mount();
  const trigger = button('新建扫描'); trigger.focus(); trigger.click(); await settle();
  const dialog = document.querySelector('[role=dialog]') as HTMLElement;
  expect(dialog.contains(document.activeElement)).toBe(true);
  document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await vi.waitFor(() => expect(document.querySelector('[role=dialog]')).toBeNull());
  expect(document.activeElement).toBe(trigger);
});


it('fetches final results even when completion arrives during an earlier page request', async () => {
  ipc.list.mockResolvedValue([{ ...scan, status: 'running', artifacts_found: 1 }]);
  let resolve!: (value: unknown) => void;
  ipc.page.mockReturnValueOnce(new Promise(finish => { resolve = finish; }));
  await mount();
  ipc.progress.mock.calls[0][0]({ payload: scan }); await settle();
  resolve({ items: [artifacts[0]], total: 1, next_cursor: null, resume_cursor: 'cursor-1' });
  await settle();
  expect(ipc.page).toHaveBeenLastCalledWith({ scanId: 'scan-a', cursor: 'cursor-1', limit: 200 });
  expect(document.querySelectorAll('tbody tr')).toHaveLength(2);
});

it('does not restore an old scan after A→B→A during scan creation', async () => {
  let finish!: (value: unknown) => void;
  ipc.start.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  await mount(); button('新建扫描').click(); await settle(); button('开始扫描').click(); await settle();
  imageId.value = 'disk-b'; await settle(); imageId.value = 'disk-a'; await settle();
  finish({ scan_id: 'old' }); await settle();
  expect(ipc.status).not.toHaveBeenCalled(); expect(document.querySelector('tbody')).toBeNull();
});
it('does not recover to a folder chosen for an old evidence view', async () => {
  let finish!: (value: string) => void; ipc.open.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  ipc.list.mockResolvedValue([scan]); await mount(); button('恢复全部').click(); await settle();
  imageId.value = 'disk-b'; await settle(); finish('D:/old'); await settle(); expect(ipc.recover).not.toHaveBeenCalled();
});

it('reads unloaded pages before reporting the complete filter result', async () => {
  ipc.list.mockResolvedValue([scan]);
  ipc.page.mockResolvedValueOnce({ items: [artifacts[0]], total: 2, next_cursor: 'next', resume_cursor: 'next' });
  await mount();
  let finish!: (value: unknown) => void;
  ipc.page.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const search = document.querySelector('input[type="search"]') as HTMLInputElement;
  search.value = 'two.pdf'; search.dispatchEvent(new Event('input'));
  search.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await settle();
  expect(ipc.page).toHaveBeenLastCalledWith({ scanId: 'scan-a', cursor: 'next', limit: 200 });
  expect(document.body.textContent).toContain('正在读取全部结果');
  expect(document.body.textContent).not.toContain('没有符合当前过滤条件的记录');
  finish({ items: [artifacts[1]], total: 2, next_cursor: null, resume_cursor: 'last' }); await settle();
  expect(document.querySelector('tbody')?.textContent).toContain('two.pdf');
  expect(document.querySelector('.footer-total')?.textContent).toContain('筛选 1');
});

it('keeps read failure separate from an empty scan and retries in place', async () => {
  ipc.list.mockResolvedValue([scan]); ipc.page.mockRejectedValueOnce(new Error('读取中断'));
  await mount();
  expect(document.querySelector('.result-read-error')?.textContent).toContain('读取中断');
  expect(document.body.textContent).not.toContain('本次扫描没有发现文件');
  button('重试').click(); await settle();
  expect(document.querySelector('.result-read-error')).toBeNull();
  expect(document.querySelector('tbody')?.textContent).toContain('one.pdf');
});

it('stops reading additional filter pages after clearing the filter', async () => {
  ipc.list.mockResolvedValue([scan]);
  ipc.page.mockResolvedValueOnce({ items: [artifacts[0]], total: 3, next_cursor: 'next', resume_cursor: 'next' });
  await mount();
  let finish!: (value: unknown) => void;
  ipc.page.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const search = document.querySelector('input[type="search"]') as HTMLInputElement;
  search.value = 'two.pdf'; search.dispatchEvent(new Event('input'));
  search.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await settle();
  (document.querySelector('[aria-label="清除筛选"]') as HTMLButtonElement).click(); await settle();
  finish({ items: [artifacts[1]], total: 3, next_cursor: 'later', resume_cursor: 'later' }); await settle();
  expect(ipc.page).toHaveBeenCalledTimes(2);
  expect(document.querySelector('tbody')?.textContent).toContain('one.pdf');
  expect(document.querySelector('tbody')?.textContent).toContain('two.pdf');
  expect(button('加载更多').disabled).toBe(false);
});

it('retains loaded rows and resumes a failed filter page on retry', async () => {
  ipc.list.mockResolvedValue([scan]);
  ipc.page.mockResolvedValueOnce({ items: [artifacts[0]], total: 2, next_cursor: 'next', resume_cursor: 'next' });
  await mount(); ipc.page.mockRejectedValueOnce(new Error('分页中断'));
  const search = document.querySelector('input[type="search"]') as HTMLInputElement;
  search.value = '.pdf'; search.dispatchEvent(new Event('input'));
  search.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })); await settle();
  expect(document.querySelector('.result-read-error')?.textContent).toContain('分页中断');
  expect(document.querySelector('tbody')?.textContent).toContain('one.pdf');
  ipc.page.mockResolvedValueOnce({ items: [artifacts[1]], total: 2, next_cursor: null, resume_cursor: 'last' });
  button('重试').click(); await settle();
  expect(ipc.page).toHaveBeenLastCalledWith({ scanId: 'scan-a', cursor: 'next', limit: 200 });
  expect(document.querySelectorAll('tbody tr')).toHaveLength(2);
  expect(document.querySelector('.result-read-error')).toBeNull();
});

it('virtualizes a large scan while selecting all filtered results for recovery', async () => {
  const many = Array.from({ length: 5000 }, (_, index) => ({ ...artifacts[0], id: `item-${index}`, name: `${index}.pdf` }));
  ipc.list.mockResolvedValue([{ ...scan, artifacts_found: many.length }]);
  ipc.page.mockResolvedValue({ items: many, total: many.length, next_cursor: null });
  await mount();
  expect(document.querySelectorAll('tbody tr:not(.virtual-spacer)').length).toBeLessThan(100);
  expect(document.querySelector('.virtual-spacer')).not.toBeNull();
  (document.querySelector('thead input') as HTMLInputElement).click(); await settle();
  button('恢复所选（5000）').click(); await settle();
  expect(ipc.recover.mock.calls[0][0].options.artifact_ids).toHaveLength(5000);
});

it('offers retry when history cannot be read instead of showing an empty history', async () => {
  ipc.list.mockRejectedValueOnce(new Error('记录不可读')); await mount();
  expect(document.querySelector('.workspace-start')?.textContent).toContain('记录不可读');
  expect(document.body.textContent).not.toContain('还没有雕刻记录');
  ipc.list.mockResolvedValue([scan]); button('重试').click(); await settle();
  expect(document.querySelector('tbody')?.textContent).toContain('one.pdf');
});
