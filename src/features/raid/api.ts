import { computed } from 'vue';
import { useRaidStack } from './composables/useRaidStack';
export { useRaidStack, registerRaidImageOpener, disposeRaidStack } from './composables/useRaidStack';
/** Case evidence grouping reads the detection cache without triggering discovery. */
export function useRaidOverview() {
  const { stack } = useRaidStack();
  return { stack: computed(() => stack.value) };
}
