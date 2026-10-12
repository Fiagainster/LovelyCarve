// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createApp, h, nextTick, ref, type App } from 'vue';
import type { SchedFunction, SchedTaskRecord, SchedTaskStatus } from '../../../lib/ipc/tasks';
import QuickAnalysisPanel from './QuickAnalysisPanel.vue';

const mocks = vi.hoisted(() => ({
  refresh: vi.fn(), create: vi.fn(), restart: vi.fn(), start: vi.fn(), remove: vi.fn(), confirm: vi.fn(), notifyInfo: vi.fn(), submitted: vi.fn(),
}));
const functions = ref<SchedFunction[]>([]);
const tasks = ref<SchedTaskRecord[]>([]);
vi.mock('../composables/useTaskCenter', async importOriginal => ({
  ...await importOriginal<typeof import('../composables/useTaskCenter')>(),
  useTaskCenter: () => ({ functions, tasks, loading: ref(false), error: ref(null), refresh: mocks.refresh, create: mocks.create, restart: mocks.restart, start: mocks.start, remove: mocks.remove }),
}));
vi.mock('../../../composables/useConfirm', () => ({ confirm: mocks.confirm }));
vi.mock('../../../composables/useToast', () => ({ notifyInfo: mocks.notifyInfo }));

/** One analysis function of plugin `name.split('/')[0]`, named `pluginName`. */
function fn(name: string, pluginName: string, displayName: string, typeName: string): SchedFunction {
  const [pluginId, analyzer] = name.split('/');
  return {
    function: name, pluginId, pluginName, pluginVersion: '1', analyzer, displayName, typeName,
    dependence: null, trigger: 'manually', internal: false, singleInstance: true, canPause: false, extensions: [], mimeTypes: [], priority: 0,
    platform: 'windows', support: [], evidenceTypes: [], evidenceLevel: true,
  };
}
function task(id: number, name: string, status: SchedTaskStatus, imageId = 'disk-a'): SchedTaskRecord {
  return {
    id, name, status, displayName: functions.value.find(item => item.function === name)?.displayName ?? name, typeName: 'baseinfo', evidenceId: 1,
    progress: 50, canPause: false, singleInstance: true, internal: false, dependence: '', spendTimeSecs: 3, remainTimeSecs: null,
    startAt: null, createAt: '2026-10-02 05:00:00', updateAt: '', result: null,
    info: { imageId, pluginId: name.split('/')[0], analyzer: 'base', trigger: 'manually', parentTid: null, error: null },
    params: { fileName: null, nodeId: null, platform: 'windows', subItems: [] },
  };
}

let app: App | undefined;
let host: HTMLDivElement;
async function settle() {
  for (let index = 0; index < 4; index++) {
    await nextTick();
    await Promise.resolve();
  }
}
async function mount() {
  app = createApp({ render: () => h(QuickAnalysisPanel, { imageId: 'disk-a', evidencePlatform: 'windows', evidenceType: 'disk', onSubmitted: mocks.submitted }) });
  app.mount(host);
  await settle();
}
const tile = (name: string) => [...host.querySelectorAll<HTMLElement>('.qa-tile')].find(element => element.querySelector('.qa-tile-name')?.textContent === name)!;
const checkbox = (name: string) => tile(name).querySelector<HTMLInputElement>('.qa-tile-choice input')!;
const status = (name: string) => tile(name).querySelector('.qa-tile-status')?.textContent ?? null;
const selectAll = () => host.querySelector<HTMLInputElement>('.qa-check-all input')!;
const categoryBox = (label: string) => host.querySelector<HTMLInputElement>(`input[aria-label="全选${label}"]`)!;
async function click(label: string) {
  const button = [...host.querySelectorAll<HTMLButtonElement>('button')].find(element => element.getAttribute('aria-label') === label || element.textContent?.trim() === label);
  if (!button) throw new Error(`Missing button: ${label}`);
  button.click();
  await settle();
}
async function tick(element: HTMLInputElement) {
  element.click();
  await settle();
}
const footer = () => host.querySelector('.qa-summary')?.textContent ?? '';

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  mocks.confirm.mockResolvedValue(true);
  mocks.remove.mockResolvedValue(true);
  mocks.create.mockImplementation(async (args: { function: string }) => ({ id: 99, name: args.function }));
  functions.value = [
    fn('chat/messages', '聊天', '消息', 'baseinfo'),
    fn('chat/contacts', '聊天', '联系人', 'baseinfo'),
    fn('qq/main', 'QQ', 'QQ 聊天记录', 'baseinfo'),
    fn('web/history', '浏览器', '浏览历史', 'browserinfo'),
    fn('web/downloads', '浏览器', '下载记录', 'browserinfo'),
    fn('log/system', '日志', '系统日志', 'loginfo'),
  ];
  tasks.value = [
    task(1, 'qq/main', 'success'),
    task(2, 'chat/messages', 'running'),
    task(3, 'qq/main', 'failed'),
    task(4, 'web/history', 'success', 'disk-b'),
  ];
  host = document.createElement('div');
  document.body.append(host);
});
afterEach(() => { app?.unmount(); app = undefined; host.remove(); });

describe('quick analysis checklist', () => {
  it('lays plugins out as tiles by category, each with its latest task on this evidence', async () => {
    await mount();
    expect(mocks.refresh).toHaveBeenCalledOnce();
    expect([...host.querySelectorAll('.qa-group-label')].map(label => label.textContent)).toEqual(['基本信息', '浏览器', '日志']);
    expect([...host.querySelectorAll('.qa-tile-name')].map(name => name.textContent)).toEqual(['聊天', 'QQ', '浏览器', '日志']);
    expect(tile('聊天').querySelector('label')?.title).toContain('包含：消息、联系人');
    expect(status('聊天')).toBe('执行中 50%');
    expect(status('QQ')).toBe('失败');
    expect(status('浏览器')).toBeNull();
    expect(checkbox('聊天').disabled).toBe(true);
    expect(checkbox('QQ').disabled).toBe(false);
    expect(host.querySelector('.qa-selectbar')?.textContent).toContain('4 个插件');
    expect(footer()).toContain('勾选要运行的插件');
  });

  it('selects everything listed with one box and a whole category with its own', async () => {
    await mount();
    await tick(selectAll());
    expect(['QQ', '浏览器', '日志'].map(name => checkbox(name).checked)).toEqual([true, true, true]);
    expect(checkbox('聊天').checked).toBe(false);
    expect(selectAll().checked).toBe(true);
    expect(footer()).toContain('已选 3 个插件');
    expect(footer()).toContain('其中 1 个将清理旧结果后重新分析');
    await tick(selectAll());
    expect(footer()).toContain('勾选要运行的插件');

    await tick(categoryBox('基本信息'));
    expect(checkbox('QQ').checked).toBe(true);
    expect(categoryBox('基本信息').checked).toBe(true);
    expect(selectAll().indeterminate).toBe(true);
    expect(host.querySelector('.qa-group-count')?.textContent).toBe('1/2');
    tasks.value = [...tasks.value, task(5, 'qq/main', 'queued')];
    await settle();
    expect(checkbox('QQ').checked).toBe(false);
    expect(categoryBox('基本信息').disabled).toBe(true);
  });

  it('narrows the select-all box to the search matches', async () => {
    await mount();
    const search = host.querySelector<HTMLInputElement>('.qa-search input')!;
    search.value = '下载';
    search.dispatchEvent(new Event('input'));
    await settle();
    expect([...host.querySelectorAll('.qa-tile-name')].map(name => name.textContent)).toEqual(['浏览器']);
    expect(host.querySelector('.qa-check-all')?.textContent).toContain('全选匹配的插件');
    await tick(selectAll());
    expect(footer()).toContain('已选 1 个插件');
  });

  it('folds a category and unfolds everything again', async () => {
    await mount();
    const toggle = () => [...host.querySelectorAll<HTMLButtonElement>('.qa-group-toggle')].find(button => button.textContent?.includes('浏览器'))!;
    expect(toggle().getAttribute('aria-expanded')).toBe('true');
    toggle().click();
    await settle();
    expect(toggle().getAttribute('aria-expanded')).toBe('false');
    await click('全部展开');
    expect(toggle().getAttribute('aria-expanded')).toBe('true');
    await click('全部收起');
    expect([...host.querySelectorAll('.qa-group-toggle')].every(button => button.getAttribute('aria-expanded') === 'false')).toBe(true);
  });
});

describe('quick analysis submission', () => {
  it('clears ended results after confirmation, runs every function of the chosen plugins and returns to the task list', async () => {
    await mount();
    await tick(checkbox('QQ'));
    await tick(checkbox('浏览器'));
    await click('开始分析');
    expect(mocks.confirm).toHaveBeenCalledWith(expect.objectContaining({ title: '任务结果已存在', message: expect.stringContaining('QQ') }));
    expect(mocks.remove).toHaveBeenCalledExactlyOnceWith(3);
    expect(mocks.create.mock.calls).toEqual([
      [{ function: 'qq/main', imageId: 'disk-a', platform: 'windows' }, { quiet: true }],
      [{ function: 'web/history', imageId: 'disk-a', platform: 'windows' }, { quiet: true }],
      [{ function: 'web/downloads', imageId: 'disk-a', platform: 'windows' }, { quiet: true }],
    ]);
    expect(mocks.notifyInfo).toHaveBeenCalledWith('快速分析', '已提交 3 个分析任务，可在任务列表查看进度');
    expect(mocks.submitted).toHaveBeenCalledOnce();
    expect(footer()).toContain('勾选要运行的插件');
  });

  it('keeps a plugin selected when one of its tasks could not be created', async () => {
    mocks.create.mockImplementation(async (args: { function: string }) => (args.function === 'web/downloads' ? null : { id: 99 }));
    await mount();
    await tick(checkbox('浏览器'));
    await tick(checkbox('日志'));
    await click('开始分析');
    expect(mocks.confirm).not.toHaveBeenCalled();
    expect(checkbox('浏览器').checked).toBe(true);
    expect(checkbox('日志').checked).toBe(false);
    expect(mocks.submitted).not.toHaveBeenCalled();
  });

  it('re-runs every evidence task from the more menu and closes the menu on outside clicks', async () => {
    tasks.value = [...tasks.value, task(7, 'log/system', 'pause')];
    await mount();
    await click('更多操作');
    expect(host.querySelector('.qa-more-menu')).not.toBeNull();
    document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }));
    await settle();
    expect(host.querySelector('.qa-more-menu')).toBeNull();
    await click('更多操作');
    await click('重新分析全部任务');
    expect(mocks.restart.mock.calls.map(([id]) => id)).toEqual([1, 2, 3]);
    expect(mocks.start).toHaveBeenCalledExactlyOnceWith(7);
  });
});
