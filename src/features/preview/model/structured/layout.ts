// 变尺寸节点的「整齐树」布局（Reingold–Tilford 思路），从左到右分层：
//   - 列（x）：按深度成列，列宽取该列所有可见节点的最大宽度，列间留 MAIN_GAP；
//   - 堆叠（y）：两趟法——先自底向上求每棵子树所需竖直高度 reqH，再自顶向下把
//     节点与其子块各自在所属竖直带内居中。子树各占不相交的竖直带，故无重叠。
// 折叠：collapsed 里的节点连同其子树整体隐藏（其父行显示 + 角标可重新展开），
// 故 childrenOf 过滤掉被折叠的子节点；根永远可见。
// 只做 LR 一个朝向——边端口锚定到「父节点的某一行」，右进左出，几何最简洁。
import type { GNode } from './graph';

export interface LayoutOpts {
  collapsed: Set<string>;
}

export interface LayoutBounds {
  width: number;
  height: number;
  /** 实际参与布局、应当渲染的节点 id（折叠后代被排除）。 */
  visible: Set<string>;
}

const MAIN_GAP = 90; // 列间距（深度方向）
const CROSS_GAP = 18; // 兄弟间距（竖直堆叠方向）
const MARGIN = 48; // 画布四周留白

export function layoutGraph(nodes: GNode[], opts: LayoutOpts): LayoutBounds {
  const { collapsed } = opts;
  const map = new Map(nodes.map((n) => [n.id, n] as const));

  const childrenOf = (n: GNode): GNode[] =>
    n.childIds
      .map((id) => map.get(id))
      .filter((c): c is GNode => c !== undefined && !collapsed.has(c.id));

  // ── 第一趟：子树所需竖直高度 ──
  const req = new Map<string, number>();
  function computeReq(n: GNode): number {
    const kids = childrenOf(n);
    let r: number;
    if (kids.length === 0) {
      r = n.height;
    } else {
      let sum = 0;
      for (const k of kids) sum += computeReq(k);
      sum += CROSS_GAP * (kids.length - 1);
      r = Math.max(n.height, sum);
    }
    req.set(n.id, r);
    return r;
  }

  const roots = nodes.filter((n) => n.parentId === null);
  for (const root of roots) computeReq(root);

  // ── 第二趟：在竖直带 [top, top+reqH] 内居中节点与子块 ──
  const visible = new Set<string>();
  const yOf = new Map<string, number>();
  function place(n: GNode, top: number): void {
    visible.add(n.id);
    const r = req.get(n.id) ?? n.height;
    yOf.set(n.id, top + (r - n.height) / 2);
    const kids = childrenOf(n);
    if (kids.length === 0) return;
    let total = 0;
    for (const k of kids) total += req.get(k.id) ?? k.height;
    total += CROSS_GAP * (kids.length - 1);
    let cursor = top + (r - total) / 2;
    for (const k of kids) {
      place(k, cursor);
      cursor += (req.get(k.id) ?? k.height) + CROSS_GAP;
    }
  }
  let rootCursor = 0;
  for (const root of roots) {
    place(root, rootCursor);
    rootCursor += (req.get(root.id) ?? root.height) + CROSS_GAP;
  }

  // ── 列位置：仅可见节点参与，每列宽 = 该列最大节点宽 ──
  let maxDepth = 0;
  for (const id of visible) maxDepth = Math.max(maxDepth, map.get(id)!.depth);
  const colW = new Array<number>(maxDepth + 1).fill(0);
  for (const id of visible) {
    const n = map.get(id)!;
    colW[n.depth] = Math.max(colW[n.depth], n.width);
  }
  const colX = new Array<number>(maxDepth + 1).fill(0);
  for (let d = 1; d <= maxDepth; d++) colX[d] = colX[d - 1] + colW[d - 1] + MAIN_GAP;

  // ── 写回坐标 + 求边界 ──
  let w = 0;
  let h = 0;
  for (const id of visible) {
    const n = map.get(id)!;
    n.x = colX[n.depth] + MARGIN;
    n.y = (yOf.get(id) ?? 0) + MARGIN;
    w = Math.max(w, n.x + n.width);
    h = Math.max(h, n.y + n.height);
  }
  return { width: w + MARGIN, height: h + MARGIN, visible };
}
