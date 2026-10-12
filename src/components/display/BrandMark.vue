<template>
  <img class="brand-mark" :class="{ 'with-background': variant === 'background' }" :src="variant === 'background' ? appIconBackground : appIcon" :width="size" :height="size" alt="" draggable="false" />
</template>

<script setup lang="ts">
import appIcon from '../../assets/app-icon.png';
import appIconBackground from '../../assets/app-icon-background.png';
withDefaults(defineProps<{ size?: number | string; variant?: 'transparent' | 'background' }>(), { size: 48, variant: 'transparent' });
</script>

<style scoped>
.brand-mark { display: block; flex-shrink: 0; object-fit: contain; }
.brand-mark.with-background { border-radius: var(--radius-lg); }
</style>
