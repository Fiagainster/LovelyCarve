import { usePersistedRef } from './usePersistedRef';

/**
 * Cross-cutting UI prefs that don't fit any one feature composable.
 * Module-level singletons so every `useUiPreferences()` caller observes the
 * same refs (toggling in settings updates all consumers without a reload).
 */

/** 界面动画 (设置 → 界面, and the titlebar user menu).
 *  - `system` → animate unless the OS asks for reduced motion (default).
 *  - `on`     → always animate, overriding the OS setting.
 *  - `off`    → switch views directly; loading spinners keep turning.
 *  `useUiMotion` turns this into the effective `data-motion` state. */
export type MotionPreference = 'system' | 'on' | 'off';

export const UI_ANIMATIONS_KEY = 'lovelycarve-ui-animations';

/** Normalizes a stored value, including the boolean the earlier on/off
 *  switch saved (`true` followed the system setting, `false` meant off). */
export function parseMotionPreference(value: unknown): MotionPreference {
  if (value === 'system' || value === 'on' || value === 'off') return value;
  return value === false ? 'off' : 'system';
}

const motionPreference = usePersistedRef<MotionPreference>(UI_ANIMATIONS_KEY, 'system');
motionPreference.value = parseMotionPreference(motionPreference.value);

/** Where a partition is projected when the user clicks 「挂载」.
 *  - `drive`  → assign a free drive letter (H–Y), the long-standing default.
 *               Best system integration (Mount Manager, Explorer drive list).
 *  - `folder` → mount into a user-picked empty NTFS folder (a directory mount
 *               point). Useful when drive letters are scarce or the analyst
 *               wants the evidence under a known case path. Dokan drops the
 *               Mount Manager flag for folder targets (its reparse-point
 *               registration is rejected for folders on many systems).
 */
export type MountTarget = 'drive' | 'folder';

const MOUNT_TARGET_KEY = 'lovelycarve-mount-target';
const mountTarget = usePersistedRef<MountTarget>(MOUNT_TARGET_KEY, 'drive');

/** Default starting directory for the folder-mount picker (设置 → 界面 → 分区
 *  挂载方式 → 文件夹). Empty string = fall back to the active case directory,
 *  then the OS default. Lets the analyst pin a fixed evidence-mount root (a
 *  dedicated NTFS scratch volume, a per-engagement share) so every folder mount
 *  opens there instead of re-deriving the default each time. Only the picker's
 *  starting location — the actual mount point is still chosen (an empty NTFS
 *  subfolder) per mount, so the user can always override. */
const MOUNT_FOLDER_ROOT_KEY = 'lovelycarve-mount-folder-root';
const mountFolderRoot = usePersistedRef<string>(MOUNT_FOLDER_ROOT_KEY, '');

/** When `true` (and mount target is `folder`), opening a case auto-loads every
 *  evidence disk and mounts all of its mountable partitions under the default
 *  mount root — one-shot "mount the whole case". Folder mode only (drive letters
 *  H–Y would be exhausted by a multi-disk case); BitLocker / locked partitions
 *  are skipped (they need a key). Default `false` — opt-in, since it loads and
 *  mounts everything on open. See `useAutoMount`. */
const MOUNT_AUTO_CASE_KEY = 'lovelycarve-mount-auto-case';
const autoMountCaseEvidence = usePersistedRef<boolean>(MOUNT_AUTO_CASE_KEY, false);

/** When `true`, removing an evidence image from a case proceeds without the
 *  destructive-action confirm dialog. Set only when the user confirms removal
 *  with 「后续不再提醒」 checked. */
const SKIP_CASE_EVIDENCE_REMOVE_CONFIRM_KEY = 'lovelycarve-skip-case-evidence-remove-confirm';
const skipCaseEvidenceRemoveConfirm = usePersistedRef<boolean>(SKIP_CASE_EVIDENCE_REMOVE_CONFIRM_KEY, false);

export function useUiPreferences() {
  return {
    motionPreference,
    mountTarget,
    mountFolderRoot,
    autoMountCaseEvidence,
    skipCaseEvidenceRemoveConfirm,
  };
}
