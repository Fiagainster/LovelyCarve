import { computed, ref, watch } from 'vue';
import type { ZipPreview, ZipPreviewEntry } from '../../../lib/ipc/zip';
import { buildZipTree, sortedChildren, type ZipTreeNode, type ZipListRow } from '../model/zipTree';
export function useZipTree(options: { report: () => ZipPreview | null; onSelect: (entry: ZipPreviewEntry) => void }) {
  const filter = ref('');
  const tree = computed(() => buildZipTree(options.report()?.entries ?? []));
  const expanded = ref<Set<string>>(new Set());

  function isOpen(node: ZipTreeNode): boolean {
    return expanded.value.has(node.path);
  }

  interface VisibleRow {
    node: ZipTreeNode;
    depth: number;
  }

  const visibleRows = computed<VisibleRow[]>(() => {
    const out: VisibleRow[] = [];
    const walk = (node: ZipTreeNode, depth: number) => {
      for (const child of sortedChildren(node)) {
        out.push({ node: child, depth });
        if (child.isDir && expanded.value.has(child.path)) walk(child, depth + 1);
      }
    };
    walk(tree.value, 0);
    return out;
  });

  // 所有目录路径（用于全展开/折叠判断）。
  const allDirPaths = computed<string[]>(() => {
    const out: string[] = [];
    const walk = (node: ZipTreeNode) => {
      for (const child of node.children.values()) {
        if (child.isDir) {
          out.push(child.path);
          walk(child);
        }
      }
    };
    walk(tree.value);
    return out;
  });

  const allExpanded = computed(() => allDirPaths.value.length > 0 && expanded.value.size >= allDirPaths.value.length);

  function toggleAll() {
    expanded.value = allExpanded.value ? new Set() : new Set(allDirPaths.value);
  }

  function toggleDir(node: ZipTreeNode) {
    const next = new Set(expanded.value);
    if (next.has(node.path)) next.delete(node.path);
    else next.add(node.path);
    expanded.value = next;
  }

  function onRowClick(node: ZipTreeNode) {
    if (node.isDir) {
      toggleDir(node);
    } else if (node.entry) {
      options.onSelect(node.entry);
    }
  }

  function onEntryClick(e: ZipPreviewEntry) {
    if (e.kind === 'dir') return;
    options.onSelect(e);
  }

  // ── 过滤（扁平匹配文件/成员路径） ────────────────────────────────────────
  // 小写路径按报告预计算一次，逐键过滤时不再为每个成员重新分配字符串。
  const searchIndex = computed(() => (options.report()?.entries ?? [])
    .filter((e) => e.kind !== 'dir')
    .map((entry) => ({ entry, haystack: entry.path.toLowerCase() })));

  const filterQuery = computed(() => filter.value.trim().toLowerCase());

  const filteredEntries = computed<ZipPreviewEntry[]>(() => {
    const q = filterQuery.value;
    if (!q) return [];
    return searchIndex.value.filter((item) => item.haystack.includes(q)).map((item) => item.entry);
  });

  const listRows = computed<ZipListRow[]>(() => (filterQuery.value
    ? filteredEntries.value.map((entry) => ({ mode: 'match' as const, key: `m:${entry.path}`, entry }))
    : visibleRows.value.map((row) => ({ mode: 'tree' as const, key: `t:${row.node.path}`, node: row.node, depth: row.depth }))));


  watch(options.report, () => {
    filter.value = '';
    expanded.value = new Set(sortedChildren(tree.value).filter(node => node.isDir).map(node => node.path));
  }, { immediate: true, flush: 'sync' });
  return { filter, filterQuery, listRows, allExpanded, toggleAll, onRowClick, onEntryClick, isOpen };
}
