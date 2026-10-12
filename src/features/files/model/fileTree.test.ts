import { expect, it } from 'vitest';
import { fileTreePathFor, visibleFileTreeRows, type TreeNodeData } from './fileTree';
const node = (id: string, parentId: string | null = null): TreeNodeData => ({ id, name: id, parentId, kind: 'dir', expanded: true, loaded: true, loading: false, childCount: 1, driveLetter: null });
it('uses render paths for duplicated node IDs and stops corrupt child cycles', () => {
  const nodes = new Map(['a', 'b', 'shared'].map(id => [id, node(id)]));
  const children = new Map([['a', ['shared']], ['b', ['shared']], ['shared', ['a']]]);
  const rows = visibleFileTreeRows(nodes, children, new Map(), ['a', 'b'], false);
  expect(rows.map(row => row.key)).toEqual(['/a', '/a/shared', '/b', '/b/shared', '/b/shared/a']);
  expect(new Set(rows.map(row => row.key)).size).toBe(rows.length);
});
it('keeps partition roots and excludes known or unknown empty child directories', () => {
  const nodes = new Map([['part:1', node('part:1')], ['empty', node('empty', 'part:1')], ['unknown', { ...node('unknown', 'part:1'), childCount: null }]]);
  expect(visibleFileTreeRows(nodes, new Map([['part:1', ['empty', 'unknown']]]), new Map([['empty', true]]), ['part:1'], true).map(row => row.node.id)).toEqual(['part:1']);
});
it('bounds malformed parent cycles when building a navigation path', () => {
  const nodes = new Map([['a', node('a', 'b')], ['b', node('b', 'a')]]);
  expect(fileTreePathFor(nodes, 'a')).toEqual([{ id: 'root', name: '/' }, { id: 'b', name: 'b' }, { id: 'a', name: 'a' }]);
});
