// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, nextTick, ref, type App } from 'vue';
import AnalysisResultsPanel from './AnalysisResultsPanel.vue';

const state = vi.hoisted(() => ({
  results: {} as Record<string, unknown>,
  exportRecords: vi.fn(),
  saveText: vi.fn(),
  notifyInfo: vi.fn(),
  notifyError: vi.fn(),
  pushProgress: vi.fn(),
  updateProgress: vi.fn(),
  dismissProgress: vi.fn(),
}));
vi.mock('../composables/useAnalysisResults', () => ({ ANALYSIS_PAGE_SIZE: 100, useAnalysisResults: () => state.results }));
vi.mock('../../../lib/ipc/client', () => ({ invokeCmd: vi.fn() }));
vi.mock('../../../lib/ipc/events', () => ({ onEvent: async () => () => {}, emitEvent: vi.fn() }));
vi.mock('../../../composables/export', () => ({ saveTextWithDialog: state.saveText }));
vi.mock('../../../composables/useToast', () => ({
  notifyInfo: state.notifyInfo,
  notifyError: state.notifyError,
  pushPersistentToast: state.pushProgress,
}));

let app: App;
let host: HTMLDivElement;
const openFile = vi.fn();
const openRecord = vi.fn();
const locating = ref(false);
const record = { artifactId: 'record', title: '用户 root', attributes: { 'source.root': 'part:1', 'source.path': 'etc/passwd', 'source.paths': ['etc/passwd', 'etc/shadow'] } };
beforeEach(() => {
  vi.resetAllMocks(); locating.value = false;
  state.pushProgress.mockReturnValue({ update: state.updateProgress, dismiss: state.dismissProgress });
  state.results = {
    tree: ref([]), treeLoading: ref(false), treeError: ref(null), dirty: ref(false),
    selectedNode: ref({ label: '用户', imageId: 'source-disk', columns: [] }),
    records: ref([record, { ...record, artifactId: 'no-source', attributes: {} }]), recordsTotal: ref(2),
    pageIndex: ref(0), pageCount: ref(1), keyword: ref(''), recordsLoading: ref(false), recordsError: ref(null),
    detail: ref(record), detailLoading: ref(false), detailError: ref(null), selectedRecordId: ref('record'),
    openRecord, setImageId: vi.fn(), ensureFresh: vi.fn(), exportRecords: state.exportRecords,
  };
  host = document.createElement('div'); document.body.append(host);
  app = createApp(AnalysisResultsPanel, { imageId: 'active-disk', get locating() { return locating.value; }, onOpenFile: openFile });
  app.mount(host);
});

describe('analysis UI feedback', () => {
  it('keeps a full field tooltip while abbreviating the cell', async () => {
    const value = '字段内容'.repeat(200);
    (state.results.selectedNode as ReturnType<typeof ref>).value.columns = [{ key: 'long', label: '长字段' }];
    (state.results.records as ReturnType<typeof ref>).value = [{ ...record, attributes: { long: value } }];
    await nextTick();
    const cell = host.querySelector<HTMLTableCellElement>('tbody td:nth-child(3)')!;
    expect(cell.title).toBe(value);
    expect(cell.textContent?.length).toBe(120);
    for (const button of host.querySelectorAll<HTMLButtonElement>('.pager button')) {
      expect(button.title).toBe(button.getAttribute('aria-label'));
    }
  });

  it('announces record loading and errors in their original location', async () => {
    (state.results.recordsLoading as ReturnType<typeof ref>).value = true;
    await nextTick();
    expect(host.querySelector('.table-scroll [role="status"]')?.textContent).toContain('正在读取记录');
    (state.results.recordsLoading as ReturnType<typeof ref>).value = false;
    (state.results.recordsError as ReturnType<typeof ref>).value = '读取失败';
    (state.results.detailError as ReturnType<typeof ref>).value = '详情失败';
    await nextTick();
    expect(host.querySelector('.table-scroll [role="alert"]')?.textContent).toContain('读取失败');
    expect(host.querySelector('.detail-dock [role="alert"]')?.textContent).toContain('详情失败');
  });

  it.each(['notice', 'toolbar'])('reports progress and cancels through the %s, then releases the notice', async source => {
    state.exportRecords.mockImplementation((_build, _write, options) => {
      options.onProgress({ loaded: 100, total: 200 });
      return new Promise((_resolve, reject) => options.signal.addEventListener('abort', () => reject(options.signal.reason), { once: true }));
    });
    host.querySelector<HTMLButtonElement>('.ui-toolbar-actions .ui-btn')!.click();
    await nextTick();
    expect(state.updateProgress).toHaveBeenCalledWith(expect.objectContaining({ progress: 50 }));
    if (source === 'notice') state.pushProgress.mock.calls[0][0].action.onClick();
    else host.querySelector<HTMLButtonElement>('.ui-toolbar-actions .ui-btn.ghost')!.click();
    await vi.waitFor(() => expect(state.dismissProgress).toHaveBeenCalledOnce());
    expect(state.saveText).not.toHaveBeenCalled();
    expect(state.notifyError).not.toHaveBeenCalled();
    expect(state.notifyInfo).toHaveBeenCalledWith('已取消导出', '用户');
    expect(host.querySelector<HTMLButtonElement>('.ui-toolbar-actions .ui-btn')!.disabled).toBe(false);
  });

  it('prevents stale result export while the directory is loading or has failed', async () => {
    const button = host.querySelector<HTMLButtonElement>('.ui-toolbar-actions .ui-btn')!;
    (state.results.treeLoading as ReturnType<typeof ref>).value = true;
    await nextTick();
    expect(button.disabled).toBe(true);
    (state.results.treeLoading as ReturnType<typeof ref>).value = false;
    (state.results.treeError as ReturnType<typeof ref>).value = '读取失败';
    await nextTick();
    expect(button.disabled).toBe(true);
    expect(host.querySelector('section > [role="status"]')?.textContent).toBe('');
  });

  it('keeps the captured result columns and releases progress after saving', async () => {
    state.exportRecords.mockImplementation(async (build, write) => {
      await write(build([record]));
      return 1;
    });
    host.querySelector<HTMLButtonElement>('.ui-toolbar-actions .ui-btn')!.click();
    await vi.waitFor(() => expect(state.dismissProgress).toHaveBeenCalledOnce());
    expect(state.saveText).toHaveBeenCalledWith(expect.objectContaining({ defaultPath: '分析结果-用户.csv', content: expect.stringContaining('用户 root') }));
    expect(state.updateProgress).toHaveBeenCalledWith(expect.objectContaining({ progress: 100, action: undefined }));
  });
});
afterEach(() => { app.unmount(); host.remove(); });

describe('analysis source actions', () => {
  it('emits a source-evidence address from the row and stops row selection', async () => {
    await nextTick();
    const button = host.querySelector<HTMLButtonElement>('tbody .ui-btn')!;
    button.click();
    expect(openFile).toHaveBeenCalledWith({ imageId: 'source-disk', rootId: 'part:1', path: 'etc/passwd' });
    expect(openRecord).not.toHaveBeenCalled();
    expect(host.querySelectorAll<HTMLButtonElement>('tbody .ui-btn')[1].disabled).toBe(true);
  });
  it('offers each source separately in details', async () => {
    await nextTick();
    const buttons = host.querySelectorAll<HTMLButtonElement>('.source-file');
    expect([...buttons].map(button => button.textContent)).toEqual(['etc/passwd', 'etc/shadow']);
    buttons[1].click();
    expect(openFile).toHaveBeenCalledWith({ imageId: 'source-disk', rootId: 'part:1', path: 'etc/shadow' });
  });
});
