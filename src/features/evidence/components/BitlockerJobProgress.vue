<template>
  <div class="bde-progress-meta">
    <span class="bde-progress-label">{{ label }}</span>
    <span class="mono bde-progress-pct">{{ percentText }}</span>
  </div>
  <div class="ui-progress bde-progress-track">
    <span :style="{ width: percent + '%' }"></span>
  </div>
  <div class="bde-progress-bytes mono">{{ bytesText }}</div>
  <div class="bde-progress-note">{{ note }}</div>
  <div class="bde-actions">
    <button
      class="ui-btn sm danger-soft"
      :disabled="cancelling"
      @click="emit('cancel')"
    >{{ cancelLabel }}</button>
  </div>
</template>
<script setup lang="ts">
defineProps<{ label: string; percent: number; percentText: string; bytesText: string; cancelling: boolean; note: string; cancelLabel: string }>();
const emit = defineEmits<{ cancel: [] }>();
</script>
<style scoped src="../styles/bitlocker-form.css"></style>
<style scoped>
.bde-progress-meta {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
}

.bde-progress-label {
  font-size: 12px;
  color: var(--text-secondary);
}

.bde-progress-pct {
  font-size: 13px;
  color: var(--text-primary);
}

.bde-progress-track {
  height: 6px;
}

.bde-progress-bytes {
  font-size: 11px;
  color: var(--text-secondary);
}

.bde-progress-note {
  font-size: 11px;
  color: var(--text-secondary);
}
</style>
