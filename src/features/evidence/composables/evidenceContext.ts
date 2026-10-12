let context: (() => unknown) | null = null;
export function registerEvidenceContext(value: () => unknown): () => void {
  context = value;
  return () => { if (context === value) context = null; };
}
export function evidenceContext() { return context?.() ?? null; }
