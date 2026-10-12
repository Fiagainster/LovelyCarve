import { computed } from 'vue';
import { useCaseSession } from './composables/useCaseSession';
export { useCaseSession, registerCaseWorkspaceTeardown, registerCaseOpened, disposeCaseSession } from './composables/useCaseSession';

/** Read-only case context for panels; switching cases remains an application workflow. */
export function useCaseContext() {
  const session = useCaseSession();
  return { activeCase: computed(() => session.activeCase.value) };
}
export function useCaseSnapshots() {
  const session = useCaseSession();
  return { activeCase: computed(() => session.activeCase.value), snapshots: computed(() => session.snapshots.value),
    buildProgress: computed(() => session.buildProgress.value), buildSnapshot: session.buildSnapshot };
}

export { useRecentCases } from './composables/useRecentCases';
export type { RecentCase } from './composables/useRecentCases';
