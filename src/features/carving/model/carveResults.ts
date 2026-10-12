import type { CarveArtifact } from '../../../lib/ipc/carve';

/** Merge incrementally paged carve findings without duplicating event races. */
export function mergeCarveArtifacts(
  current: readonly CarveArtifact[],
  incoming: readonly CarveArtifact[],
): CarveArtifact[] {
  if (incoming.length === 0) return [...current];
  const byId = new Map(current.map(artifact => [artifact.id, artifact]));
  for (const artifact of incoming) byId.set(artifact.id, artifact);
  return [...byId.values()].sort(
    (left, right) => left.offset - right.offset || left.id.localeCompare(right.id),
  );
}
