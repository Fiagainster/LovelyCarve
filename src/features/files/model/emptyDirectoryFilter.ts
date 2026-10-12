/**
 * Decide whether a directory may be rendered while "hide empty directories"
 * is enabled. Unknown state stays hidden until either the recursive snapshot
 * result or the direct child count arrives, preventing empty folders from
 * flashing during an asynchronous reload.
 */
export function shouldShowDirectoryWithEmptyFilter(
  subtreeEmpty: boolean | undefined,
  childCount: number | null | undefined,
): boolean {
  if (subtreeEmpty !== undefined) return !subtreeEmpty;
  return childCount != null && childCount > 0;
}
