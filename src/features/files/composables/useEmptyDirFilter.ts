import type { Ref } from 'vue';
import { usePersistedRef } from '../../../composables/usePersistedRef';

/**
 * 「隐藏空目录」开关——目录树（`DiskFileTree`）与文件列表（`DiskFileList`）共用
 * 同一个**模块级单例** ref，所以在目录树标题栏切换会同时影响两个面板（否则两个
 * 组件各持一个 `usePersistedRef`，同会话内互不联动）。语义：开启后，凡是「整棵
 * 子树里没有任何文件」的目录都隐藏——既含本身没条目的空目录，也含里面只有空
 * 目录（递归到任意深度都没文件）的目录。经 localStorage 记住，跨会话生效。
 */
const hideEmptyDirs = usePersistedRef('lovelycarve-tree-hide-empty-dirs', false);

export function useEmptyDirFilter(): { hideEmptyDirs: Ref<boolean> } {
  return { hideEmptyDirs };
}
