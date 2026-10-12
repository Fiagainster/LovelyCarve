import type { AnalysisResultNode } from '../../../lib/ipc/analysisResults';

export interface AnalysisResultGroup {
  id: string;
  label: string;
  category: string;
  results: AnalysisResultNode[];
}

/** Flatten redundant evidence/category levels while keeping function ownership. */
export function buildAnalysisGroups(tree: AnalysisResultNode[], imageId: string, query = ''): AnalysisResultGroup[] {
  const groups = new Map<string, AnalysisResultGroup>();
  const search = query.trim().toLocaleLowerCase();
  function visit(nodes: AnalysisResultNode[], category = '', owner?: AnalysisResultNode) {
    for (const node of nodes) {
      if (node.kind === 'result' && node.imageId === imageId) {
        const id = owner?.nodeId ?? `${imageId}::${node.analyzer}`;
        const label = owner?.label ?? node.analyzer;
        if (search && !`${category} ${label} ${node.label}`.toLocaleLowerCase().includes(search)) continue;
        const group = groups.get(id) ?? { id, label, category, results: [] };
        group.results.push(node);
        groups.set(id, group);
      } else {
        visit(node.children, node.kind === 'class' ? node.label : category, node.kind === 'function' ? node : owner);
      }
    }
  }
  visit(tree);
  return [...groups.values()];
}
