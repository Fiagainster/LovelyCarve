import { ref, watch, type Ref } from 'vue';

/**
 * A ref that persists its value to localStorage.
 * Falls back to the provided default if nothing is stored.
 *
 * Gracefully no-ops in environments without `localStorage` (vitest node
 * mode, SSR) so module-level `usePersistedRef(...)` declarations don't
 * throw at import time when the test runner pulls the composable in
 * transitively.
 */
export function usePersistedRef<T>(key: string, defaultValue: T): Ref<T> {
  const hasStorage = typeof localStorage !== 'undefined';
  let initial = defaultValue;

  if (hasStorage) {
    const stored = localStorage.getItem(key);
    if (stored !== null) {
      try {
        initial = JSON.parse(stored);
      } catch {
        initial = defaultValue;
      }
    }
  }

  const data = ref(initial) as Ref<T>;

  watch(data, (newVal) => {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(key, JSON.stringify(newVal));
    }
  }, { deep: true });

  return data;
}
