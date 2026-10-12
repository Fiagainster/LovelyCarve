<template>
  <Teleport to="body">
    <Transition name="ui-modal">
      <div
        v-if="active"
        class="ui-modal-overlay confirm-overlay"
        role="dialog"
        aria-modal="true"
        @click.self="onCancel"
        @keydown.esc="onCancel"
      >
        <div class="ui-modal confirm-modal" :class="{ danger: active.danger }">
          <div class="confirm-body">
            <span class="ui-modal-icon" :class="{ danger: active.danger }">
              <component :is="active.danger ? AlertTriangle : HelpCircle" :size="18" />
            </span>
            <div class="confirm-copy">
              <h2 class="confirm-title">{{ active.title }}</h2>
              <p class="confirm-message">{{ active.message }}</p>
              <label v-if="active.checkbox" class="ui-check confirm-option">
                <input
                  type="checkbox"
                  :checked="active.checkboxChecked"
                  @change="onCheckboxChange"
                />
                <span>{{ active.checkbox.label }}</span>
              </label>
            </div>
          </div>
          <footer class="ui-modal-footer">
            <button
              type="button"
              class="ui-btn"
              @click="onCancel"
            >{{ active.cancelText }}</button>
            <button
              ref="confirmBtnRef"
              type="button"
              class="ui-btn"
              :class="active.danger ? 'danger' : 'primary'"
              @click="onConfirm"
            >{{ active.confirmText }}</button>
          </footer>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from 'vue';
import { AlertTriangle, HelpCircle } from 'lucide-vue-next';
import { useConfirmHost } from '../../composables/useConfirm';

const { active, onConfirm, onCancel, setCheckboxChecked } = useConfirmHost();
const confirmBtnRef = ref<HTMLButtonElement | null>(null);

// Auto-focus the confirm button on open so Enter submits and Esc
// cancels via the global keydown — keyboard parity with `window.confirm`.
watch(active, async (val) => {
  if (val) {
    await nextTick();
    confirmBtnRef.value?.focus();
  }
});

function onCheckboxChange(event: Event) {
  setCheckboxChecked((event.target as HTMLInputElement).checked);
}

// Global Esc to cancel (the @keydown.esc on overlay only fires if the
// overlay has focus, which it doesn't by default).
function onKeydown(e: KeyboardEvent) {
  if (!active.value) return;
  if (e.key === 'Escape') {
    e.preventDefault();
    onCancel();
  } else if (e.key === 'Enter') {
    // Only catch Enter outside textareas / inputs so the user's typing
    // never accidentally confirms.
    const target = e.target as HTMLElement | null;
    if (target && /^(TEXTAREA|INPUT)$/.test(target.tagName)) return;
    e.preventDefault();
    onConfirm();
  }
}
import { onBeforeUnmount, onMounted } from 'vue';
onMounted(() => window.addEventListener('keydown', onKeydown));
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown));
</script>

<style scoped>
/* Above every other dialog: confirmations can be raised from inside one. */
.confirm-overlay {
  z-index: 13000;
}

.confirm-modal {
  width: min(440px, 100%);
}

.confirm-body {
  display: flex;
  align-items: flex-start;
  gap: 14px;
  padding: 20px 22px 18px;
}

.confirm-copy {
  flex: 1;
  min-width: 0;
  padding-top: 2px;
}

.confirm-title {
  margin: 0;
  color: var(--text-primary);
  font-size: 15px;
  font-weight: 600;
  line-height: 1.45;
}

.confirm-message {
  margin: 6px 0 0;
  color: var(--text-secondary);
  font-size: 13px;
  line-height: 1.65;
  white-space: pre-wrap;
  word-break: break-word;
}

.confirm-option {
  margin-top: 12px;
  color: var(--text-secondary);
}

.confirm-option:hover {
  color: var(--text-primary);
}
</style>
