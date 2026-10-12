<template>
  <button
    ref="trigger"
    type="button"
    class="ui-select"
    role="combobox"
    aria-haspopup="listbox"
    :aria-expanded="open"
    :aria-controls="open ? listId : undefined"
    :aria-activedescendant="open && highlighted >= 0 ? optionId(highlighted) : undefined"
    :disabled="disabled"
    @click="toggle"
    @keydown="onKeydown"
  >
    <!-- Every label shares one grid cell: the field keeps the width of the
         longest option, like a native select, and only the current one shows. -->
    <span class="ui-select-value">
      <span
        v-for="(option, index) in options"
        :key="index"
        :class="{ 'ui-select-sizer': index !== selectedIndex }"
        :aria-hidden="index === selectedIndex ? undefined : 'true'"
      >{{ option.label }}</span>
    </span>
    <Teleport to="body">
      <div
        v-if="open"
        :id="listId"
        ref="list"
        class="ui-menu ui-select-list"
        popover="manual"
        role="listbox"
        @mousedown.prevent
      >
        <div
          v-for="(option, index) in options"
          :id="optionId(index)"
          :key="index"
          class="ui-menu-item ui-select-option"
          role="option"
          :aria-selected="index === selectedIndex"
          :aria-disabled="option.disabled ? 'true' : undefined"
          :data-highlighted="index === highlighted ? '' : undefined"
          @mousemove="option.disabled || (highlighted = index)"
          @click="choose(index)"
        >{{ option.label }}</div>
      </div>
    </Teleport>
  </button>
</template>

<script lang="ts">
export interface UiSelectOption<T> {
  value: T;
  label: string;
  disabled?: boolean;
}
</script>

<script setup lang="ts" generic="T extends string | number | null">
import { computed, nextTick, onBeforeUnmount, onDeactivated, ref, useId, watch } from 'vue';
import { placeDropdown } from '../../utils/dropdownPlacement';

/** The app's dropdown. The trigger keeps the `ui-select` field look; the list is
 *  a `ui-menu` in the top layer, so it matches the theme instead of the system
 *  popup. Keyboard follows the select-only combobox pattern (focus stays on the
 *  trigger, the highlight moves through aria-activedescendant). */
const props = defineProps<{ options: readonly UiSelectOption<T>[]; disabled?: boolean }>();
const model = defineModel<T>({ required: true });
const emit = defineEmits<{ change: [value: T] }>();

const PAGE_STEP = 10;
const TYPEAHEAD_RESET_MS = 500;

const trigger = ref<HTMLButtonElement | null>(null);
const list = ref<HTMLDivElement | null>(null);
const open = ref(false);
const highlighted = ref(-1);
const listId = useId();
const selectedIndex = computed(() => props.options.findIndex(option => Object.is(option.value, model.value)));
let typed = '';
let typedAt = 0;

watch(() => props.disabled, disabled => { if (disabled) close(); });
watch(() => props.options, () => {
  if (!open.value) return;
  if (highlighted.value >= props.options.length) highlighted.value = lastEnabled();
  void nextTick(place);
});
onDeactivated(close);
onBeforeUnmount(close);

function optionId(index: number): string {
  return `${listId}-${index}`;
}

/** First enabled option at or past `start` going `direction`, or -1. */
function enabledFrom(start: number, direction: 1 | -1): number {
  for (let index = start; index >= 0 && index < props.options.length; index += direction) {
    if (!props.options[index].disabled) return index;
  }
  return -1;
}
function firstEnabled(): number { return enabledFrom(0, 1); }
function lastEnabled(): number { return enabledFrom(props.options.length - 1, -1); }
function startIndex(): number {
  const selected = selectedIndex.value;
  return selected >= 0 && !props.options[selected].disabled ? selected : firstEnabled();
}

function toggle(): void {
  if (open.value) {
    close();
    return;
  }
  // WebKit does not focus a clicked button, and the keyboard needs the focus.
  trigger.value?.focus();
  void openList(startIndex());
}

async function openList(index: number): Promise<void> {
  if (open.value || props.disabled || !props.options.length) return;
  open.value = true;
  highlighted.value = index;
  await nextTick();
  const element = list.value;
  const anchor = trigger.value;
  if (!open.value || !element || !anchor) return;
  // The top layer keeps the list above dialogs and outside clipping containers.
  if (typeof element.showPopover === 'function') element.showPopover();
  element.style.fontFamily = getComputedStyle(anchor).fontFamily;
  place();
  reveal();
  document.addEventListener('pointerdown', onOutsidePointer, true);
  document.addEventListener('scroll', onScroll, { capture: true, passive: true });
  window.addEventListener('resize', close);
  window.addEventListener('blur', close);
}

function close(): void {
  if (!open.value) return;
  open.value = false;
  highlighted.value = -1;
  document.removeEventListener('pointerdown', onOutsidePointer, true);
  document.removeEventListener('scroll', onScroll, true);
  window.removeEventListener('resize', close);
  window.removeEventListener('blur', close);
}

function place(): void {
  const element = list.value;
  const anchor = trigger.value;
  if (!element || !anchor) return;
  const rect = anchor.getBoundingClientRect();
  element.style.minWidth = `${rect.width}px`;
  element.style.maxHeight = 'none';
  // Offset sizes are layout sizes, unaffected by the entrance animation's scale.
  const placement = placeDropdown(
    rect,
    { width: element.offsetWidth, height: element.offsetHeight },
    { width: window.innerWidth, height: window.innerHeight },
  );
  element.style.top = `${placement.top}px`;
  element.style.left = `${placement.left}px`;
  element.style.maxHeight = `${placement.maxHeight}px`;
  // ui-pop-in (motion.css) rises from the trigger's side.
  element.style.setProperty('--ui-pop-offset', placement.above ? '4px' : '-4px');
}

/** Scrolls the list, and only the list, to the highlighted option. */
function reveal(): void {
  const element = list.value;
  const option = element?.querySelector<HTMLElement>('[data-highlighted]');
  if (!element || !option) return;
  if (option.offsetTop < element.scrollTop) {
    element.scrollTop = option.offsetTop;
  } else if (option.offsetTop + option.offsetHeight > element.scrollTop + element.clientHeight) {
    element.scrollTop = option.offsetTop + option.offsetHeight - element.clientHeight;
  }
}

function highlight(index: number): void {
  if (index < 0) return;
  highlighted.value = index;
  void nextTick(reveal);
}

function moveBy(delta: number): void {
  const direction = delta > 0 ? 1 : -1;
  const target = Math.max(0, Math.min(props.options.length - 1, highlighted.value + delta));
  const next = enabledFrom(target, direction);
  highlight(next >= 0 ? next : enabledFrom(target, direction === 1 ? -1 : 1));
}

function choose(index: number): void {
  const option = props.options[index];
  if (!option || option.disabled) return;
  close();
  trigger.value?.focus();
  if (Object.is(option.value, model.value)) return;
  model.value = option.value;
  emit('change', option.value);
}

function chooseHighlighted(): void {
  if (highlighted.value >= 0) choose(highlighted.value);
  else close();
}

function onOutsidePointer(event: PointerEvent): void {
  const target = event.target;
  if (target instanceof Node && (trigger.value?.contains(target) || list.value?.contains(target))) return;
  close();
}

function onScroll(event: Event): void {
  if (event.target instanceof Node && list.value?.contains(event.target)) return;
  close();
}

function openingIndex(key: string): number | null {
  switch (key) {
    case 'ArrowDown':
    case 'ArrowUp':
    case 'Enter':
    case ' ':
      return startIndex();
    case 'Home':
      return firstEnabled();
    case 'End':
      return lastEnabled();
    default:
      return null;
  }
}

function onKeydown(event: KeyboardEvent): void {
  if (!open.value) {
    const index = openingIndex(event.key);
    if (index !== null) {
      event.preventDefault();
      void openList(index);
    } else if (isTypeahead(event)) {
      event.preventDefault();
      const key = event.key;
      void openList(startIndex()).then(() => { if (open.value) typeahead(key); });
    }
    return;
  }
  switch (event.key) {
    case 'ArrowDown': moveBy(1); break;
    case 'ArrowUp': moveBy(-1); break;
    case 'PageDown': moveBy(PAGE_STEP); break;
    case 'PageUp': moveBy(-PAGE_STEP); break;
    case 'Home': highlight(firstEnabled()); break;
    case 'End': highlight(lastEnabled()); break;
    case 'Enter': chooseHighlighted(); break;
    case ' ':
      if (typed && Date.now() - typedAt < TYPEAHEAD_RESET_MS) typeahead(' ');
      else chooseHighlighted();
      break;
    case 'Escape':
      // Only the list closes; a dialog around it stays open.
      event.stopPropagation();
      close();
      break;
    case 'Tab':
      // Commit and let the focus move on.
      chooseHighlighted();
      return;
    default:
      if (!isTypeahead(event)) return;
      typeahead(event.key);
  }
  event.preventDefault();
}

function isTypeahead(event: KeyboardEvent): boolean {
  return event.key.length === 1 && event.key !== ' ' && !event.ctrlKey && !event.metaKey && !event.altKey;
}

/** Highlights the next option whose label starts with what was typed;
 *  repeating one character cycles through the options starting with it. */
function typeahead(key: string): void {
  const now = Date.now();
  typed = now - typedAt > TYPEAHEAD_RESET_MS ? key : typed + key;
  typedAt = now;
  const query = typed.toLocaleLowerCase();
  const repeated = [...query].every(char => char === query[0]);
  const prefix = repeated ? query[0] : query;
  const count = props.options.length;
  const start = Math.max(0, highlighted.value + (repeated ? 1 : 0));
  for (let offset = 0; offset < count; offset += 1) {
    const index = (start + offset) % count;
    const option = props.options[index];
    if (!option.disabled && option.label.toLocaleLowerCase().startsWith(prefix)) {
      highlight(index);
      return;
    }
  }
}
</script>

<style scoped>
.ui-select-value {
  display: grid;
  min-width: 0;
}

.ui-select-value > span {
  grid-area: 1 / 1;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.ui-select-sizer {
  visibility: hidden;
}

/* Keyboard focus shows the field ring from ui.css, not the global button outline. */
.ui-select:focus-visible {
  outline: none;
}

/* Normally in the top layer through the Popover API; the fixed layer and
   z-index only matter where that API is missing. */
.ui-select-list {
  position: fixed;
  inset: auto;
  z-index: 9100;
  margin: 0;
  max-width: calc(100vw - 16px);
  overflow-y: auto;
  overscroll-behavior: contain;
}

.ui-select-option {
  font-family: inherit;
}

/* One highlight for mouse and keyboard: a resting pointer does not keep a
   second one after the arrow keys move on. */
.ui-select-option:hover:not([data-highlighted], [aria-selected='true']) {
  background: transparent;
}
</style>
