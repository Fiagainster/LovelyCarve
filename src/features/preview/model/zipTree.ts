import type { ZipPreviewEntry } from '../../../lib/ipc/zip';
export interface ZipTreeNode {
  name: string;
  path: string;
  isDir: boolean;
  entry: ZipPreviewEntry | null;
  children: Map<string, ZipTreeNode>;
}

function newNode(name: string, path: string, isDir: boolean): ZipTreeNode {
  return { name, path, isDir, entry: null, children: new Map() };
}

export function buildZipTree(entries: readonly ZipPreviewEntry[]): ZipTreeNode {
  const root = newNode('', '', true);
  for (const e of entries) {
    const segs = e.path.split('/').filter(Boolean);
    let cur = root;
    for (let i = 0; i < segs.length; i++) {
      const seg = segs[i];
      const isLast = i === segs.length - 1;
      const segPath = segs.slice(0, i + 1).join('/');
      let child = cur.children.get(seg);
      if (!child) {
        child = newNode(seg, segPath, !isLast || e.kind === 'dir');
        cur.children.set(seg, child);
      }
      if (isLast) {
        child.entry = e;
        // 「目录优先」：ZIP 允许同时存在文件成员 `a` 与 `a/` 下的成员（规范化后路径
        // 都是 `a`）。绝不把已有子节点 / 中间段 / 显式目录的节点降级成文件——否则
        // 其子树会在树里失联。冲突的文件成员仍可经过滤框（扁平匹配）访问。
        child.isDir = child.isDir || child.children.size > 0 || e.kind === 'dir';
      } else {
        child.isDir = true;
      }
      cur = child;
    }
  }
  return root;
}

// dirs first, then case-insensitive name order.
export function sortedChildren(node: ZipTreeNode): ZipTreeNode[] {
  return [...node.children.values()].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  });
}


export type ZipListRow =
  | { mode: 'match'; key: string; entry: ZipPreviewEntry }
  | { mode: 'tree'; key: string; node: ZipTreeNode; depth: number };
