import { shallowRef } from 'vue';

interface AnalysisContext {
  caseDirectory: () => string | null;
  completedRuns: () => readonly string[];
}
const context = shallowRef<AnalysisContext | null>(null);
export function registerAnalysisContext(value: AnalysisContext): () => void {
  context.value = value;
  return () => { if (context.value === value) context.value = null; };
}
export function analysisCaseDirectory() { return context.value?.caseDirectory() ?? null; }
export function analysisCompletedRuns() { return context.value?.completedRuns() ?? []; }
