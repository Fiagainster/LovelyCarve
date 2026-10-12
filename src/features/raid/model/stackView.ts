import type { RaidStack, RaidArray, RaidArrayMember, RaidMemberOrigin, RaidVolumeGroup } from '../../../lib/ipc/raid';
import { formatBinaryBytes as formatBytes } from '../../../utils/format';
export { formatBytes };

/** VGs with at least one PV on this image. */
export function relevantRaidVolumeGroups(stack: RaidStack | null, imageId: string): RaidVolumeGroup[] {
  return stack?.volume_groups.filter(vg => vg.members.some(m => m.image_id === imageId)) ?? [];
}

/** Arrays this image participates in: a whole-disk member lives on it, an
 *  LV member's volume group has a PV on it, or the image IS the assembled
 *  synthetic device (its info page shows the array it came from). */
export function relevantRaidArrays(s: RaidStack | null, imageId: string): RaidArray[] {
  if (!s) return [];
  const vgOnImage = new Set(
    s.volume_groups
      .filter(vg => vg.members.some(m => m.image_id === imageId))
      .map(vg => vg.vg_uuid),
  );
  return s.arrays.filter(
    arr =>
      arr.assembled_image_id === imageId ||
      arr.members.some(m =>
        m.origin.kind === 'whole' ? m.origin.image_id === imageId : vgOnImage.has(m.origin.vg_uuid),
      ),
  );
}

export function arrayTitle(arr: RaidArray): string {
  return arr.set_name || `md/${arr.set_uuid.slice(0, 12)}`;
}

/** Tooltip for the metadata-less badge: the parameters the heuristic
 *  recovered, so an examiner can see the reconstruction was inferred. */
export function heuristicTitle(arr: RaidArray): string {
  const layoutNames = ['左异步', '右异步', '左同步', '右同步'];
  const parts = ['无 RAID 元数据，参数经文件系统校验推定', `块大小 ${arr.chunksize_sectors / 2}K`];
  if (arr.level === 5) parts.push(`布局 ${layoutNames[arr.layout] ?? `#${arr.layout}`}`);
  parts.push(`数据偏移 ${arr.data_offset_sectors} 扇区`);
  return parts.join(' · ');
}

export function activeMembers(arr: RaidArray): number {
  return arr.members.filter(m => m.slot !== null).length;
}

type StatusSeverity = 'success' | 'warn' | 'info';

export function arrayStatus(arr: RaidArray): { label: string; sev: StatusSeverity } {
  if (arr.assembled_image_id) return { label: '已重组', sev: 'success' };
  if (activeMembers(arr) < arr.raid_disks) {
    return { label: `成员不全 ${activeMembers(arr)}/${arr.raid_disks}`, sev: 'warn' };
  }
  if (arr.level !== 0 && arr.level !== 5) {
    return { label: `RAID${arr.level} 暂不支持重组`, sev: 'info' };
  }
  // mdadm layouts 0..3 are the only RAID5 variants the assembler replays.
  if (arr.level === 5 && arr.layout > 3) {
    return { label: `RAID5 布局 ${arr.layout} 暂不支持重组`, sev: 'info' };
  }
  // Remaining causes (filesystem not recognised, member geometry conflicts,
  // probe read errors) are only distinguishable via the diagnostics list.
  return { label: '未能重组，见诊断', sev: 'warn' };
}

/** Relative capacity of a member vs. the largest column (0..100), for the thin
 *  micro-gauge under each disk chip. Pure presentation — surfaces relative
 *  member sizes from data already on screen; never affects layout/logic. */
export function gaugePct(arr: RaidArray, m: RaidArrayMember): number {
  const max = Math.max(...arr.members.map(mm => mm.data_size_sectors), 1);
  return Math.max(4, Math.round((m.data_size_sectors / max) * 100));
}

/** Members that hold a column, in slot order (col0 → colN). Spares/faulty
 *  rows (null slot) are dropped — the chain shows the stripe layout only. */
export function orderedMembers(arr: RaidArray): RaidArrayMember[] {
  return arr.members.filter(m => m.slot != null).sort((a, b) => (a.slot ?? 0) - (b.slot ?? 0));
}

/** Compact disk name for a member: the .dd basename for whole-disk members,
 *  the LV name for LV-backed ones. */
export function memberShortLabel(m: RaidArrayMember, imagePaths: ReadonlyMap<string, string>): string {
  if (m.origin.kind === 'lv') return m.origin.lv_name;
  const p = imagePaths.get(m.origin.image_id);
  return p ? baseName(p) : m.origin.image_id.slice(0, 8);
}

// Display names for mdadm RAID5/6 layouts 0..3 (Left/Right × Asymmetric/
// Symmetric). The 异步/同步 wording is the convention domestic forensic / RAID
// rebuild tools use: 异步 = asymmetric, 同步 = symmetric.
const LAYOUT_NAMES = ['左异步', '右异步', '左同步', '右同步'];
const LEVEL_NAMES: Record<number, string> = {
  0: 'RAID 0（条带）',
  1: 'RAID 1（镜像）',
  5: 'RAID 5（单校验）',
  6: 'RAID 6（双校验）',
  10: 'RAID 10（条带镜像）',
};

export function levelLabel(arr: RaidArray): string {
  return LEVEL_NAMES[arr.level] ?? `RAID ${arr.level}`;
}

export function layoutLabel(arr: RaidArray): string {
  return LAYOUT_NAMES[arr.layout] ?? `布局 #${arr.layout}`;
}

/** Chunk (stripe unit) size — `chunksize_sectors` is in 512-byte sectors. */
export function chunkDisplay(arr: RaidArray): string {
  const kib = arr.chunksize_sectors / 2;
  return kib >= 1024 ? `${kib / 1024} MiB` : `${kib} KiB`;
}

/** Per-member reserved header before the array data starts. */
export function dataOffsetDisplay(arr: RaidArray): string {
  const s = arr.data_offset_sectors;
  if (s === 0) return '0 扇区';
  return `${s.toLocaleString()} 扇区 · ${formatBytes(s * 512)}`;
}

/** The stripe diagram only makes sense for the striped/parity levels we model
 *  (0/5/6); mirrors and other levels have no chunk layout to draw. */
export function showDiagram(arr: RaidArray): boolean {
  return arr.level === 0 || arr.level === 5 || arr.level === 6;
}

/** Compact per-slot column labels for the diagram (slot index → disk name). */
export function diskLabelsFor(arr: RaidArray, imagePaths: ReadonlyMap<string, string>): (string | null)[] {
  const labels: (string | null)[] = new Array(arr.raid_disks).fill(null);
  for (const m of arr.members) {
    if (m.slot == null || m.slot < 0 || m.slot >= arr.raid_disks) continue;
    labels[m.slot] = memberShortLabel(m, imagePaths);
  }
  return labels;
}

/** Slot of the dropped/parity-rebuilt column, if the heuristic recovered one. */
export function missingSlotFor(arr: RaidArray): number | null {
  const m = arr.members.find(mm => mm.reconstructed && mm.slot != null);
  return m && m.slot != null ? m.slot : null;
}

export function chunkLabelFor(arr: RaidArray): string {
  return `${arr.chunksize_sectors / 2}K`;
}

export function vgSummary(vg: RaidVolumeGroup): string {
  const lvs = vg.logical_volumes.map(lv => `${lv.name} ${formatBytes(lv.size_bytes)}`).join('、');
  return lvs ? `${vg.members.length} PV · LV ${lvs}` : `${vg.members.length} PV`;
}

function baseName(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || path;
}

export function originTitle(origin: RaidMemberOrigin, imagePaths: ReadonlyMap<string, string>): string {
  if (origin.kind === 'lv') return `逻辑卷 ${origin.vg_name}/${origin.lv_name}（VG ${origin.vg_uuid}）`;
  const path = imagePaths.get(origin.image_id) ?? origin.image_id;
  return `${path} · 分区${origin.partition_id} · 偏移 0x${origin.byte_offset.toString(16).toUpperCase()}`;
}

/** Suggested RAW filename for a reconstructed array: array name (or a short
 *  set-uuid fallback) + RAID level, with Windows-illegal chars stripped. */
export function exportBaseName(arr: RaidArray): string {
  const raw = arr.set_name || `md-${arr.set_uuid.slice(0, 12)}`;
  const safe = raw.replace(/[\\/:*?"<>|]/g, '_').replace(/\s+/g, '_');
  return `${safe || 'raid'}_RAID${arr.level}`;
}
