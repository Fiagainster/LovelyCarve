import type { AnalysisRecord } from '../../../lib/ipc/analysisResults';
import type { FileLocationTarget } from '../../../lib/models/fileLocation';

export interface AnalysisSource { label: string; target: FileLocationTarget }
const text = (value: unknown): string => typeof value === 'string' && value.trim() ? value : '';

/** Provenance is an evidence address; labels and artifact IDs are never node IDs. */
export function analysisSources(record: AnalysisRecord, imageId: string): AnalysisSource[] {
  imageId = record.imageId || imageId;
  const attributes = record.attributes;
  const rootId = text(attributes['source.root']) || text(attributes['linux.root']);
  const path = text(attributes['source.path']);
  const declared = Array.isArray(attributes['source.paths']) ? attributes['source.paths'].filter((value): value is string => typeof value === 'string' && !!value.trim()) : [];
  const legacy = text(attributes['linux.source']);
  const legacyPaths = legacy.startsWith('systemd/') ? [] : legacy.split(';').filter(value => value.includes('/'));
  const paths = [...new Set(path ? [path, ...declared] : declared.length ? declared : legacyPaths)];
  if (rootId && paths.length) return paths.map(path => ({ label: path, target: { imageId, rootId, path } }));
  const nodeId = text(attributes['source.node']) || text(record.sourceNodeId);
  return nodeId ? [{ label: path || nodeId, target: { imageId, nodeId } }] : [];
}
