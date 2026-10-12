import { beforeEach, describe, expect, it } from 'vitest';

import type { SchedFunction } from '../../../lib/ipc/tasks';
import {
  buildPlatforms,
  buildPluginGroups,
  favouriteNames,
  groupByPlugin,
  isFavouritePlugin,
  preprocessFunctions,
  toggleFavouritePlugin,
  useQuickAnalysisSelection,
} from './useQuickAnalysis';

function fn(overrides: Partial<SchedFunction> = {}): SchedFunction {
  return {
    function: 'org.test/demo',
    pluginId: 'org.test',
    pluginName: 'Demo',
    pluginVersion: '1',
    analyzer: 'demo',
    displayName: '演示功能',
    typeName: 'baseinfo',
    dependence: null,
    trigger: 'manually',
    internal: false,
    singleInstance: true,
    canPause: false,
    extensions: [],
    mimeTypes: [],
    priority: 0,
    platform: 'linux',
    support: ['系统信息'],
    evidenceTypes: [],
    evidenceLevel: true,
    ...overrides,
  };
}

describe('buildPlatforms', () => {
  it('derives ordered platform column and prefers the evidence platform', () => {
    const functions = [
      fn({ function: 'a/linux', platform: 'linux' }),
      fn({ function: 'b/android', platform: 'android' }),
      fn({ function: 'c/win', platform: 'windows' }),
      fn({ function: 'd/all', platform: null }),
    ];
    const { platforms, defaultPlatform } = buildPlatforms(functions, 'windows');
    // "all" 功能克隆到每个平台,因此平台列是全集;预选检材平台。
    expect(platforms).toEqual(['android', 'harmony', 'ios', 'windows', 'macos', 'linux', 'iot']);
    expect(defaultPlatform).toBe('windows');
    expect(buildPlatforms(functions, null).defaultPlatform).toBe('android');
    expect(buildPlatforms([], 'linux')).toEqual({ platforms: [], defaultPlatform: '' });
  });
});

describe('preprocessFunctions', () => {
  const functions = [
    fn({ function: 'internal/one', internal: true, platform: 'linux' }),
    fn({ function: 'linux/only', platform: 'linux', evidenceTypes: ['disk'] }),
    fn({ function: 'linux/file', platform: 'linux', evidenceLevel: false }),
    fn({ function: 'all/clone', platform: null }),
    fn({ function: 'win/only', platform: 'windows' }),
  ];

  it('drops internal and file-level functions and clones all-platform functions per platform', () => {
    const linux = preprocessFunctions(functions, { platform: 'linux' });
    expect(linux.map((item) => item.function)).toEqual(['linux/only', 'all/clone']);
    const windows = preprocessFunctions(functions, { platform: 'windows' });
    expect(windows.map((item) => item.function)).toEqual(['all/clone', 'win/only']);
  });

  it('treats a manifest platform of "any" as every platform', () => {
    const anyPlatform = [fn({ function: 'git/repos', platform: 'any' }), fn({ function: 'win/only', platform: 'windows' })];
    expect(buildPlatforms(anyPlatform, 'windows').platforms).not.toContain('any');
    expect(preprocessFunctions(anyPlatform, { platform: 'windows' }).map((item) => item.function)).toEqual(['git/repos', 'win/only']);
    expect(preprocessFunctions(anyPlatform, { platform: 'linux' }).map((item) => item.function)).toEqual(['git/repos']);
  });

  it('filters by evidence type when the function declares one', () => {
    const disk = preprocessFunctions(functions, { platform: 'linux', evidenceType: 'disk' });
    expect(disk.map((item) => item.function)).toEqual(['linux/only', 'all/clone']);
    const backup = preprocessFunctions(functions, { platform: 'linux', evidenceType: 'compressfile' });
    expect(backup.map((item) => item.function)).toEqual(['all/clone']);
  });
});

describe('plugin groups', () => {
  const plugin = (id: string, name: string, typeName: string, analyzers: Array<[string, string]>) =>
    analyzers.map(([analyzer, displayName]) =>
      fn({ function: `${id}/${analyzer}`, pluginId: id, pluginName: name, analyzer, displayName, typeName }));
  const functions = [
    ...plugin('org.user', 'Windows用户痕迹', 'baseinfo', [['wps', '最近访问的WPS文件'], ['desktop', '用户桌面']]),
    ...plugin('org.mail', '邮件分析', 'fileanalyzer', [['mail', '邮件']]),
    ...plugin('org.hash', '全盘哈希', 'longtime', [['hash', '哈希比对']]),
    ...plugin('org.todesk', 'ToDesk配置登记', 'baseinfo', [['config', 'ToDesk配置与clientId声明']]),
    ...plugin('org.evtx', 'Windows完整事件日志', 'system', [['evtx', '事件日志']]),
  ];

  it('merges the functions of one plugin into a single entry in list order', () => {
    const plugins = groupByPlugin(functions);
    expect(plugins.map((item) => item.id)).toEqual(['org.user', 'org.mail', 'org.hash', 'org.todesk', 'org.evtx']);
    expect(plugins[0]).toMatchObject({ name: 'Windows用户痕迹', typeName: 'baseinfo' });
    expect(plugins[0].functions.map((item) => item.displayName)).toEqual(['最近访问的WPS文件', '用户桌面']);
  });

  it('groups favourites first, then types with Chinese labels, longtime last', () => {
    const groups = buildPluginGroups(groupByPlugin(functions), { favourites: ['org.todesk'], search: '' });
    expect(groups.map((group) => [group.key, group.label])).toEqual([
      ['favour', '收藏'], ['baseinfo', '基本信息'], ['system', '系统记录'], ['fileanalyzer', '文件分析'], ['longtime', '耗时任务'],
    ]);
    expect(groups[0].plugins.map((item) => item.id)).toEqual(['org.todesk']);
    expect(groups[1].plugins.map((item) => item.id)).toEqual(['org.user', 'org.todesk']);
  });

  it('finds a plugin by its name, id or any of its functions', () => {
    const plugins = groupByPlugin(functions);
    const ids = (search: string) =>
      buildPluginGroups(plugins, { favourites: [], search }).flatMap((group) => group.plugins.map((item) => item.id));
    expect(ids('todesk')).toEqual(['org.todesk']);
    expect(ids('用户桌面')).toEqual(['org.user']);
    expect(ids('org.mail/mail')).toEqual(['org.mail']);
    expect(ids('不存在')).toEqual([]);
  });
});

describe('favourites', () => {
  beforeEach(() => {
    favouriteNames.value = [];
  });

  it('toggles a plugin and still honours entries saved per function', () => {
    expect(isFavouritePlugin('org.user')).toBe(false);
    toggleFavouritePlugin('org.user');
    expect(isFavouritePlugin('org.user')).toBe(true);
    expect(isFavouritePlugin('org.use')).toBe(false);
    toggleFavouritePlugin('org.user');
    expect(isFavouritePlugin('org.user')).toBe(false);

    favouriteNames.value = ['org.user/wps', 'org.mail'];
    expect(isFavouritePlugin('org.user')).toBe(true);
    toggleFavouritePlugin('org.user');
    expect(favouriteNames.value).toEqual(['org.mail']);
  });
});

describe('useQuickAnalysisSelection', () => {
  it('tracks whole plugins by id', () => {
    const selection = useQuickAnalysisSelection();
    selection.toggle('org.user');
    expect(selection.isSelected('org.user')).toBe(true);
    expect(selection.selectionList.value).toEqual(['org.user']);
    selection.toggle('org.user');
    expect(selection.isSelected('org.user')).toBe(false);
    selection.toggle('org.user');
    selection.clear();
    expect(selection.selectionList.value).toEqual([]);
  });

  it('selects and clears a category in the order plugins were ticked', () => {
    const selection = useQuickAnalysisSelection();
    selection.toggle('org.hash');
    selection.selectMany(['org.user', 'org.hash']);
    expect(selection.selectionList.value).toEqual(['org.hash', 'org.user']);
    selection.deselectMany(['org.user', 'org.hash']);
    expect(selection.selectionList.value).toEqual([]);
  });
});
