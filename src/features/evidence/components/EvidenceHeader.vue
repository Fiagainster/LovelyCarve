<template>
<header class="evidence-head">
  <span class="evidence-icon" aria-hidden="true"><component :is="evidenceIcon" :size="20" /></span>
  <div class="evidence-text">
    <div class="evidence-title">
      <h2 class="evidence-name" :title="info.path">{{ fileName }}</h2>
      <span class="ui-badge sm mono">{{ formatLabel }}</span>
    </div>
    <p v-if="info.source_format !== 'assembled'" class="evidence-path">
      <span class="mono">{{ info.path }}</span>
      <button class="ui-icon-btn sm" title="复制路径" aria-label="复制路径" @click="emit('copy', '路径', info.path)"><Copy :size="12" /></button>
    </p>
    <p class="evidence-size">
      <strong class="mono">{{ formatBytes(info.size_bytes) }}</strong>
      <template v-if="onDiskSummary"> · 实际占用 <span class="mono">{{ onDiskSummary.size }}</span>（{{ onDiskSummary.ratio }}）
</template>
          </p>
        </div>
        <AutoDetectBadge class="evidence-platform" :image-id="info.id" />
      </header>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Archive, Boxes, Container, Copy, FolderOpen, HardDrive, History, LockOpen } from 'lucide-vue-next';
import type { ImageInfo } from '../../../lib/ipc/image';
import { evidenceDisplayName, formatBytes } from '../model/evidenceOverview';
import AutoDetectBadge from './AutoDetectBadge.vue';
const props = defineProps<{ info: ImageInfo }>();
const emit = defineEmits<{ copy: [label: string, value: string] }>();

// ── 检材头部 ─────────────────────────────────────────────────────────────
const fileName = computed(() => evidenceDisplayName(props.info));

/** 格式徽章：镜像格式写代号（VMDK、EWF），其余写中文名。 */
const FORMAT_LABELS: Record<string, string> = {
  sevenz: '7z',
  docker: 'Docker',
  folder: '文件夹',
  physical_disk: '物理磁盘',
  xiaomi_firmware: '小米固件',
  ldplayer: '雷电模拟器备份',
  assembled: 'RAID 重组',
  decrypted: '解密视图',
  shadow_copy: 'VSS 卷影副本',
};
const formatLabel = computed(() => {
  if (props.info.ios_backup) return 'iOS 备份';
  const format = props.info?.source_format ?? '';
  return FORMAT_LABELS[format] ?? format.toUpperCase();
});

const evidenceIcon = computed(() => {
  switch (props.info?.source_format) {
    case 'folder':
      return FolderOpen;
    case 'zip':
    case 'tar':
    case 'sevenz':
    case 'ad1':
      return Archive;
    case 'docker':
      return Container;
    case 'decrypted':
      return LockOpen;
    case 'shadow_copy':
      return History;
    case 'assembled':
      return Boxes;
    default:
      return HardDrive;
  }
});

/** 实际占用只在与逻辑大小不同时显示（压缩、稀疏格式）。 */
const onDiskSummary = computed(() => {
  const info = props.info;
  if (!info?.on_disk_bytes || !info.size_bytes || info.on_disk_bytes === info.size_bytes) return null;
  return {
    size: formatBytes(info.on_disk_bytes),
    ratio: `${((info.on_disk_bytes / info.size_bytes) * 100).toFixed(1)}%`,
  };
});


</script>

<style scoped>
/* ── 检材头部 ── */
.evidence-head {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: start;
  gap: 8px 14px;
  padding: 4px 4px 0;
}
.evidence-icon { display: grid; place-items: center; width: 40px; height: 40px; border-radius: var(--radius-md); background: var(--sev-info-bg); color: var(--primary-color); }
.evidence-text { min-width: 0; }
.evidence-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
.evidence-name { min-width: 0; margin: 0; overflow: hidden; font-size: 16px; font-weight: 600; line-height: 1.4; text-overflow: ellipsis; white-space: nowrap; }
.evidence-path { display: flex; align-items: center; gap: 2px; margin: 2px 0 0; color: var(--text-secondary); font-size: 11px; line-height: 1.6; }
.evidence-path .mono { min-width: 0; overflow-wrap: anywhere; }
.evidence-path .ui-icon-btn { flex-shrink: 0; }
.evidence-size { margin: 2px 0 0; color: var(--text-secondary); font-size: 12px; }
.evidence-size strong { color: var(--text-primary); font-size: 13px; font-weight: 600; font-variant-numeric: tabular-nums; }
.evidence-platform { padding-top: 6px; }

@container image-info (max-width: 640px) {
  .evidence-head { grid-template-columns: auto minmax(0, 1fr); }
  .evidence-platform { grid-column: 2; justify-content: flex-start; padding-top: 0; }
}
</style>
