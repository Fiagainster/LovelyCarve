import { describe, expect, it } from 'vitest';
import { analysisSources } from './analysisSource';
import type { AnalysisRecord } from '../../../lib/ipc/analysisResults';
const record = (attributes: Record<string, unknown>): AnalysisRecord => ({ artifactId: 'artifact', runId: 'run', artifactType: 'test.record', title: 'Title', value: null, attributes });

describe('plugin source addresses', () => {
  it('prefers the volume-relative path to an inode so container sources retain their view', () => {
    expect(analysisSources(record({ 'source.root': 'host-docker:container:', 'source.path': 'etc/passwd', 'source.node': 'ext4:part:0:42' }), 'disk-b'))
      .toEqual([{ label: 'etc/passwd', target: { imageId: 'disk-b', rootId: 'host-docker:container:', path: 'etc/passwd' } }]);
  });
  it('lists multiple sources and preserves literal semicolons in standard paths', () => {
    expect(analysisSources(record({ 'source.root': 'part:1', 'source.path': 'file;name', 'source.paths': ['file;name', 'etc/shadow'] }), 'disk'))
      .toEqual(['file;name', 'etc/shadow'].map(path => ({ label: path, target: { imageId: 'disk', rootId: 'part:1', path } })));
  });
  it('supports older Linuxinfo paths and excludes display-only provenance labels', () => {
    expect(analysisSources(record({ 'linux.root': 'part:0', 'linux.source': 'etc/passwd;etc/shadow' }), 'disk').map(source => source.label))
      .toEqual(['etc/passwd', 'etc/shadow']);
    expect(analysisSources(record({ 'linux.root': 'part:0', 'linux.source': 'dpkg' }), 'disk')).toEqual([]);
    expect(analysisSources(record({ 'linux.root': 'part:0', 'linux.source': 'systemd/multi-user.target.wants' }), 'disk')).toEqual([]);
    expect(analysisSources(record({ 'source.path': 'etc/passwd' }), 'disk')).toEqual([]);
  });
  it('uses explicit node provenance when no path address is available', () => {
    expect(analysisSources(record({ 'source.node': 'node' }), 'disk')).toEqual([{ label: 'node', target: { imageId: 'disk', nodeId: 'node' } }]);
    expect(analysisSources(record({ 'source.node': 42 }), 'disk')).toEqual([]);
  });
  it('uses persisted ownership and the input node for file plugins without provenance', () => {
    expect(analysisSources({ ...record({}), imageId: 'source-disk', sourceNodeId: 'input-file' }, 'active-disk'))
      .toEqual([{ label: 'input-file', target: { imageId: 'source-disk', nodeId: 'input-file' } }]);
  });
});
