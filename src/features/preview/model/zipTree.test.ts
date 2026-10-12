import { expect, it } from 'vitest';
import { buildZipTree, sortedChildren } from './zipTree';
import type { ZipPreviewEntry } from '../../../lib/ipc/zip';
const entry = (path: string, kind: ZipPreviewEntry['kind'] = 'file') => ({ path, name: path, kind } as ZipPreviewEntry);
it('keeps a directory reachable when file and directory names collide in either order', () => {
  for (const entries of [[entry('a'), entry('a/child')], [entry('a/child'), entry('a')]]) {
    const tree = buildZipTree(entries); const parent = tree.children.get('a')!;
    expect(parent.isDir).toBe(true); expect(parent.children.get('child')?.entry?.path).toBe('a/child');
  }
});
it('synthesizes missing ancestor directories and sorts directories before files', () => {
  const tree = buildZipTree([entry('z/file'), entry('beta'), entry('Alpha'), entry('a/child')]);
  expect(sortedChildren(tree).map(node => node.path)).toEqual(['a', 'z', 'Alpha', 'beta']);
});
