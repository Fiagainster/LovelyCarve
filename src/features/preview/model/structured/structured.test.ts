import { describe, expect, it } from 'vitest';
import { parseStructured, type PValue } from './parse';
import { buildGraph } from './graph';
import { layoutGraph } from './layout';

describe('parseStructured', () => {
  it('parses JSON into an ordered PValue tree', () => {
    const r = parseStructured('{"a":1,"b":{"c":2},"d":[10,20]}', 'json');
    expect(r.error).toBeUndefined();
    expect(r.format).toBe('json');
    expect(r.value?.t).toBe('object');
    const obj = r.value as Extract<PValue, { t: 'object' }>;
    expect(obj.entries.map(([k]) => k)).toEqual(['a', 'b', 'd']);
    expect(obj.entries[2][1]).toEqual({ t: 'array', items: [{ t: 'number', v: 10 }, { t: 'number', v: 20 }] });
  });

  it('tolerates JSONC comments and trailing commas', () => {
    const r = parseStructured('{\n  // first\n  "a": 1, /* mid */\n  "b": 2,\n}', 'json');
    expect(r.error).toBeUndefined();
    const obj = r.value as Extract<PValue, { t: 'object' }>;
    expect(obj.entries.map(([k]) => k)).toEqual(['a', 'b']);
  });

  it('parses YAML scalars, maps and sequences', () => {
    const r = parseStructured('name: test\nnested:\n  x: 1\nlist:\n  - a\n  - b\n', 'yaml');
    expect(r.error).toBeUndefined();
    const obj = r.value as Extract<PValue, { t: 'object' }>;
    expect(obj.entries.map(([k]) => k)).toEqual(['name', 'nested', 'list']);
    expect(obj.entries[2][1]).toEqual({ t: 'array', items: [{ t: 'string', v: 'a' }, { t: 'string', v: 'b' }] });
  });

  it('reports an error for malformed input instead of throwing', () => {
    const r = parseStructured('a: 1\na: 2\n', 'yaml'); // duplicate mapping key
    expect(r.value).toBeNull();
    expect(r.error).toBeTruthy();
    expect(r.error).not.toBe('empty');
  });

  it('flags empty documents', () => {
    expect(parseStructured('   \n  ', 'json').error).toBe('empty');
  });

  it('breaks YAML anchor cycles instead of hanging', () => {
    const r = parseStructured('a: &x\n  b: *x\n', 'yaml');
    expect(r.error).toBeUndefined();
    const obj = r.value as Extract<PValue, { t: 'object' }>;
    const inner = obj.entries[0][1] as Extract<PValue, { t: 'object' }>;
    expect(inner.entries[0][1]).toEqual({ t: 'string', v: '[circular]' });
  });

  it('keeps a primitive root as a single value', () => {
    const r = parseStructured('"hello"', 'json');
    expect(r.value).toEqual({ t: 'string', v: 'hello' });
  });

  // XML 走原生 DOMParser，node 测试环境没有 DOM——仅在有 DOM 时运行。
  const hasDom = typeof DOMParser !== 'undefined';
  (hasDom ? it : it.skip)('parses XML elements, attributes and repeated tags', () => {
    const r = parseStructured('<root id="1"><item>a</item><item>b</item></root>', 'xml');
    const obj = r.value as Extract<PValue, { t: 'object' }>;
    expect(obj.entries[0][0]).toBe('root');
  });
});

describe('buildGraph', () => {
  it('emits one node per container and edges from container rows', () => {
    const v = parseStructured('{"a":1,"b":{"c":2},"d":[10,20]}', 'json').value!;
    const g = buildGraph(v);
    expect(g.nodes).toHaveLength(3); // root + {c:2} + [10,20]
    expect(g.edges).toHaveLength(2);
    expect(g.nodeCount).toBe(3);
    expect(g.overCap).toBe(false);

    const root = g.nodes.find((n) => n.parentId === null)!;
    expect(root.kind).toBe('object');
    expect(root.rows.map((r) => r.key)).toEqual(['a', 'b', 'd']);
    expect(root.rows[0].childId).toBeNull(); // primitive row, inline
    expect(root.rows[1].childId).not.toBeNull(); // container row, linked
    // 边从「那一行」引出
    const childEdge = g.edges.find((e) => e.from === root.id && e.to === root.rows[1].childId);
    expect(childEdge?.fromRowIndex).toBe(1);
  });

  it('renders a primitive root as a single dashed node', () => {
    const g = buildGraph(parseStructured('42', 'json').value!);
    expect(g.nodes).toHaveLength(1);
    expect(g.nodes[0].kind).toBe('primitive');
    expect(g.edges).toHaveLength(0);
  });
});

describe('layoutGraph', () => {
  it('assigns coordinates and a positive canvas size', () => {
    const g = buildGraph(parseStructured('{"a":1,"b":{"c":2}}', 'json').value!);
    const b = layoutGraph(g.nodes, { collapsed: new Set() });
    expect(b.width).toBeGreaterThan(0);
    expect(b.height).toBeGreaterThan(0);
    expect(b.visible.size).toBe(2);
    for (const id of b.visible) {
      const n = g.nodes.find((x) => x.id === id)!;
      expect(Number.isFinite(n.x)).toBe(true);
      expect(Number.isFinite(n.y)).toBe(true);
    }
  });

  it('hides a collapsed subtree from the visible set', () => {
    const g = buildGraph(parseStructured('{"a":1,"b":{"c":2},"d":[10,20]}', 'json').value!);
    const root = g.nodes.find((n) => n.parentId === null)!;
    const collapsedChild = root.rows[1].childId!; // {c:2}
    const b = layoutGraph(g.nodes, { collapsed: new Set([collapsedChild]) });
    expect(b.visible.has(collapsedChild)).toBe(false);
    expect(b.visible.has(root.id)).toBe(true);
    expect(b.visible.size).toBe(2); // root + [10,20]
  });
});
