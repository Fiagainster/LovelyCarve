<template>
  <section class="file-detail-section" :class="{ wide }" :aria-label="title">
    <details v-if="collapsible" v-disclosure-motion class="ui-disclosure">
      <summary class="section-title"><ChevronRight :size="13" class="ui-chevron" />{{ title }}</summary>
      <div class="section-content"><slot /></div>
    </details>
    <template v-else>
      <h4 class="section-title">{{ title }}</h4>
      <div class="section-content"><slot /></div>
    </template>
  </section>
</template>

<script setup lang="ts">
import { ChevronRight } from 'lucide-vue-next';
import { vDisclosureMotion } from '../../composables/uiMotionDirectives';
defineProps<{ title: string; collapsible?: boolean; wide?: boolean }>();
</script>

<style scoped>
.file-detail-section { min-width: 0; }
.file-detail-section.wide { grid-column: 1 / -1; }
.section-title { margin: 0; padding-bottom: 9px; border-bottom: 1px solid var(--border-color); color: var(--text-secondary); font-size: 11px; font-weight: 600; line-height: 18px; }
summary.section-title:hover { color: var(--text-primary); }
.section-content { padding-top: 8px; }
</style>
