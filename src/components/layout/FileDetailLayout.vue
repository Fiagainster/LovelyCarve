<template>
  <div class="file-detail-layout">
    <header class="file-summary">
      <span v-if="$slots.icon" class="file-summary-icon"><slot name="icon" /></span>
      <div class="file-summary-text">
        <h3 :class="{ mono: monoTitle }">{{ title }}</h3>
        <div v-if="$slots.badges" class="file-summary-badges"><slot name="badges" /></div>
      </div>
    </header>
    <div v-if="$slots.actions" class="file-detail-actions"><slot name="actions" /></div>
    <div class="file-detail-sections"><slot /></div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ title: string; monoTitle?: boolean }>();
</script>

<style scoped>
.file-detail-layout { padding: 18px 16px 24px; color: var(--text-primary); container: file-detail / inline-size; }
.file-summary { display: flex; align-items: flex-start; gap: 12px; min-width: 0; }
.file-summary-icon { display: grid; place-items: center; flex: none; width: 36px; height: 36px; border: 1px solid var(--border-color); border-radius: var(--radius-md); background: var(--surface-primary); color: var(--text-secondary); }
.file-summary-text { flex: 1; min-width: 0; }
.file-summary h3 { margin: 0; font-size: 14px; font-weight: 600; line-height: 1.6; overflow-wrap: anywhere; user-select: text; }
.file-summary h3.mono { font-family: var(--font-mono); font-size: 13px; }
.file-summary-badges { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin-top: 6px; }
.file-detail-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 14px; }
.file-detail-sections { display: grid; grid-template-columns: minmax(0, 1fr); gap: 20px; margin-top: 20px; }
@container file-detail (min-width: 640px) {
  .file-detail-sections { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 24px; }
}
</style>
