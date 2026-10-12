
import { type NavItem, type NavGroup } from './types';


/** Categories for the workspace rail and their second-level pages. */
export function buildImageNavGroups(options: { supportsRawAnalysis?: boolean } = {}): NavGroup[] {
  const toolItems: NavItem[] = [
    { id: 'search-results', icon: 'Search', label: '全局搜索', badge: null, badgeType: '' },
    { id: 'timeline', icon: 'BarChart3', label: '时间线', badge: null, badgeType: '' },
    ...(options.supportsRawAnalysis === false
      ? []
      : [{ id: 'carve', icon: 'FileSearch', label: '文件雕刻', badge: null, badgeType: '' }]),
  ];
  return [
    {
      groupId: 'image',
      groupLabel: '镜像',
      groupIcon: 'HardDrive',
      groupColor: 'var(--primary-color)',
      items: [
        { id: 'dashboard', icon: 'Info', label: '镜像信息', badge: null, badgeType: '' },
        { id: 'files', icon: 'Files', label: '文件列表', badge: null, badgeType: '' },
      ],
    },
    {
      groupId: 'tools',
      groupLabel: '工具',
      groupIcon: 'Wrench',
      groupColor: 'var(--primary-color)',
      items: toolItems,
    },
    {
      groupId: 'analysis',
      groupLabel: '分析',
      groupIcon: 'ScanSearch',
      groupColor: 'var(--primary-color)',
      items: [{ id: 'analysis', icon: 'ScanSearch', label: '分析结果', badge: null, badgeType: '' }],
    },
  ];
}
