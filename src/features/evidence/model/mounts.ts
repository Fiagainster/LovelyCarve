import type { DokanMountView } from '../../../lib/ipc/mount';

// Compact label for the mounted-state pill: drive letter (`H:`) as-is, folder
// mount → just its last path segment (full path stays in the button title).
export function mountLabel(view: DokanMountView): string {
  const mp = view.mount_point.replace(/[\\/]+$/, '');
  if (/^[A-Za-z]:$/.test(mp)) return mp;
  const seg = mp.split(/[\\/]/).filter(Boolean).pop();
  return seg || mp;
}
