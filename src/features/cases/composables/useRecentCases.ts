// Recently-opened cases, persisted to localStorage.
//
// A case is uniquely identified by its directory, so we key on `dir`. The list
// is a module-level singleton (shared across every caller) and is recorded from
// `useCaseSession` on every create/open, so it stays current no matter which
// entry point opened the case. The start page renders it for one-click re-open.
import type { Ref } from 'vue';

import { usePersistedRef } from '../../../composables/usePersistedRef';

export interface RecentCase {
  /** Case directory — the unique key. */
  dir: string;
  /** Display name — the case's manifest name (set at create time, read back
   *  on open). For UI-created cases this equals the directory's last segment. */
  name: string;
  /** ISO timestamp of the last open. */
  openedAt: string;
}

const STORAGE_KEY = 'lovelycarve.recentCases';
/** Keep only the most recent N entries. */
const MAX_RECENT = 8;

const recentCases: Ref<RecentCase[]> = usePersistedRef<RecentCase[]>(STORAGE_KEY, []);

export function useRecentCases() {
  /** Record (or bump to the top) a freshly opened/created case. */
  function recordCase(dir: string, name: string): void {
    const rest = recentCases.value.filter((c) => c.dir !== dir);
    rest.unshift({ dir, name, openedAt: new Date().toISOString() });
    recentCases.value = rest.slice(0, MAX_RECENT);
  }

  /** Drop an entry (e.g. its directory no longer exists). */
  function removeCase(dir: string): void {
    recentCases.value = recentCases.value.filter((c) => c.dir !== dir);
  }

  /** Clear only local history; case directories and registry entries stay intact. */
  function clearRecentCases(): void {
    recentCases.value = [];
  }

  return { recentCases, recordCase, removeCase, clearRecentCases };
}
