<template>
  <div :ref="popover.controlEl" class="bde-control" @click.stop>
    <span class="ui-badge mono bde-status" :class="locked ? 'is-locked' : 'info is-unlocked'" :title="statusTitle">
      <Lock v-if="locked" :size="11" />
      <LockOpen v-else :size="11" />
      <span>{{ locked ? 'LOCKED' : 'UNLOCKED' }}</span>
    </span>

    <!-- In-progress chip: visible whenever a job is running for this
         partition, even if the user closed the popover. Click reopens it. -->
    <button
      v-if="job"
      class="bde-chip"
      :title="job.cancelling ? '正在取消…' : '点击查看解锁进度'"
      @click="openPopover"
    >
      <span class="bde-chip-dot"></span>
      <span>{{ job.cancelling ? '取消中' : '解锁中' }}</span>
      <span class="bde-chip-pct">{{ unlockProgress.percentText }}</span>
    </button>
    <button
      v-else-if="scanJob"
      class="bde-chip"
      :title="scanJob.cancelling ? '正在取消…' : '点击查看扫描进度'"
      @click="openPopover"
    >
      <span class="bde-chip-dot"></span>
      <span>{{ scanJob.cancelling ? '取消中' : '扫描中' }}</span>
      <span class="bde-chip-pct">{{ scanProgress.percentText }}</span>
    </button>
    <button
      v-else-if="locked"
      class="ui-btn sm bde-btn"
      :class="{ active: popoverOpen }"
      @click="togglePopover"
    >
      <KeyRound :size="13" />
      <span>解锁</span>
    </button>
    <button
      v-else
      class="ui-btn sm bde-btn"
      :disabled="!!exportJob"
      :title="exportJob ? '正在导出解密后完整镜像' : '导出完整 RAW 镜像，并将此 BitLocker 分区替换为解密后内容'"
      @click="exportDecryptedImage"
    >
      <Download :size="13" />
      <span>{{ exportJob ? '导出中' : '导出解密镜像' }}</span>
    </button>

    <!-- Teleported to <body> so the host panel's `overflow-y: auto`
         (which also clips overflow-x) can't cut off the wide popover when
         the card sits in a right-hand grid column. Positioned via fixed
         coords computed from the control's bounding rect. -->
    <Teleport to="body">
      <div
        v-if="popoverOpen"
        :ref="popover.popoverEl"
        class="bde-popover"
        @click.stop
      >
        <div class="bde-popover-head">
        <span class="bde-popover-icon"><KeyRound :size="14" /></span>
        <div class="bde-popover-title">解锁 BitLocker 分区</div>
      </div>

      <!-- Mode tabs — hidden while any job is running so the progress UI
           replaces the input fields cleanly. -->
      <div v-if="!job && !scanJob" class="ui-seg bde-tabs" role="tablist">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="ui-seg-item bde-tab"
          :class="{ 'is-active': mode === tab.id }"
          role="tab"
          :aria-selected="mode === tab.id"
          @click="mode = tab.id"
        >{{ tab.label }}</button>
      </div>

      <BitlockerCredentialForm v-if="!job && !scanJob && (mode === 'password' || mode === 'recovery')" :mode="mode" v-model:password-input="passwordInput" v-model:recovery-input="recoveryInput"
        :elements="credentialElements" :recovery-error="recoveryError" :recovery-note-id="recoveryNoteId" :valid-groups="recoveryCheck.validGroups"
        :can-submit-password="canSubmitPassword" :can-submit-recovery="canSubmitRecovery" @password="submitPassword" @recovery="submitRecovery" @blur="onRecoveryBlur" @close="closePopover" />

      <BitlockerKeyForm v-if="!job && !scanJob && mode === 'key'" :key="`${imageId}:${partition.id}`" @submit="submitKey" @close="closePopover" />
      <p v-if="metadata && !job && !scanJob" class="ui-field-hint">元数据 v{{ metadata.version }} · {{ metadata.bytes_per_sector }} 字节扇区{{ metadata.used_space_only ? ' · 仅加密已用空间' : '' }} · 保护器 {{ metadata.protector_types.map(protectorLabel).join('、') || '无' }}</p>

      <BitlockerMemoryCandidates v-if="!job && !scanJob && mode === 'memory'" :memory-path="memoryPath" :candidates="candidates"
        @pick="pickMemoryFile" @scan="startScan" @reset="resetMemory" @close="closePopover" @unlock="unlockWithCandidate" />

      <BitlockerJobProgress v-if="job" :label="job.cancelling ? '取消中…' : unlockLabel" :percent="unlockProgress.percent" :percent-text="unlockProgress.percentText" :bytes-text="unlockProgress.bytesText"
        :cancelling="job.cancelling" note="解锁任务会在后台继续，关闭面板不会中断。" cancel-label="终止任务" @cancel="onCancelUnlock" />
      <BitlockerJobProgress v-if="!job && scanJob" :label="scanJob.cancelling ? '取消中…' : '扫描内存镜像'" :percent="scanProgress.percent" :percent-text="scanProgress.percentText" :bytes-text="scanProgress.bytesText"
        :cancelling="scanJob.cancelling" note="扫描会在后台继续，关闭面板不会中断。" cancel-label="终止扫描" @cancel="onCancelScan" />
      </div>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
import { computed, onScopeDispose, ref, watch } from 'vue';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { Lock, LockOpen, KeyRound, Download } from 'lucide-vue-next';
import { useBitlockerCredentials } from '../composables/useBitlockerCredentials';
import { useBitlockerPopover } from '../composables/useBitlockerPopover';
import { dismissBitlockerReminder } from '../composables/useFileSession';
import type { DiskPartition } from '../../../lib/ipc/image';
import {
  cancelBitlockerMemoryScan,
  cancelBitlockerUnlock,
  startBitlockerMemoryScan,
  startBitlockerPasswordUnlock,
  startBitlockerRecoveryUnlock,
  startBitlockerVmkUnlock,
  startBitlockerKeyUnlock,
  useBitlockerJob,
  useBitlockerScanJob,
} from '../composables/useBitlockerUnlocks';
import { useImageExport } from '../composables/useImageExport';
import { bitlockerStatus, bitlockerMetadata, type BitlockerMetadata } from '../../../lib/ipc/bitlocker';
import { bitlockerProgress, type BitlockerInputMode as Mode, type BitlockerVmkCandidate, type BitlockerKeyCredential } from '../model/bitlocker';
import BitlockerCredentialForm from './BitlockerCredentialForm.vue';
import BitlockerKeyForm from './BitlockerKeyForm.vue';
import BitlockerMemoryCandidates from './BitlockerMemoryCandidates.vue';
import BitlockerJobProgress from './BitlockerJobProgress.vue';

const props = defineProps<{
  imageId: string;
  partition: DiskPartition;
}>();

const emit = defineEmits<{
  (e: 'unlocked', partitionId: number): void;
}>();

const tabs: { id: Mode; label: string }[] = [
  { id: 'password', label: '密码' },
  { id: 'recovery', label: '恢复密钥' },
  { id: 'memory', label: '内存镜像' },
  { id: 'key', label: '其他密钥' },
];

const locked = ref(true);
const metadata = ref<BitlockerMetadata | null>(null);
const encryption = ref<string | null>(null);
const mode = ref<Mode>('recovery');

const { passwordInput, recoveryInput, passwordEl, recoveryEl, credentialElements, canSubmitPassword, recoveryCheck, recoveryTouched, recoveryNoteId, recoveryError, canSubmitRecovery, onRecoveryBlur, clearCredentials } = useBitlockerCredentials();

const memoryPath = ref<string>('');
const candidates = ref<BitlockerVmkCandidate[] | null>(null);

const job = useBitlockerJob(
  () => props.imageId,
  () => props.partition.id,
);
const scanJob = useBitlockerScanJob(
  () => props.imageId,
  () => props.partition.id,
);
const { startBitlockerDecryptedImageExport, jobFor } = useImageExport();
const exportJob = computed(() => jobFor(props.imageId));

const statusTitle = computed(() => {
  if (locked.value) return 'BitLocker 卷未解锁';
  return encryption.value
    ? `BitLocker 已解锁 · ${encryption.value}`
    : 'BitLocker 已解锁';
});

const unlockLabel = computed(() => {
  const j = job.value;
  if (!j) return '解锁中';
  switch (j.mode) {
    case 'password':
      return '密码解锁中';
    case 'recovery':
      return '恢复密钥解锁中';
    case 'vmk':
      return 'VMK 解锁中';
    case 'key':
      return '密钥解锁中';
  }
});

const unlockProgress = computed(() => bitlockerProgress(job.value));
const scanProgress = computed(() => bitlockerProgress(scanJob.value));

const popover = useBitlockerPopover({
  context: () => `${props.imageId}:${props.partition.id}`, mode: () => mode.value, content: () => [candidates.value, !!job.value, !!scanJob.value],
  dismissReminder: () => dismissBitlockerReminder(props.imageId), focusInput: focusActiveInput,
  onClose: () => { if (!job.value && !scanJob.value) clearCredentials(); },
});
const { popoverOpen, openPopover, closePopover, togglePopover } = popover;
let generation = 0;
let statusRequest = 0;
let disposed = false;
const current = (captured: number) => !disposed && captured === generation;
onScopeDispose(() => { disposed = true; generation += 1; });

async function refreshStatus() {
  const captured = generation;
  const request = ++statusRequest;
  const imageId = props.imageId;
  const partitionId = props.partition.id;
  try {
    const status = await bitlockerStatus({
      imageId,
      partitionId,
    });
    if (!current(captured) || request !== statusRequest) return;
    const wasLocked = locked.value;
    locked.value = status.locked;
    encryption.value = status.encryption_method;
    if (wasLocked && !status.locked) {
      emit('unlocked', partitionId);
    }
  } catch {
    // Keep the last known lock state when the status query is unavailable.
  }
}

function protectorLabel(value: number): string {
  const labels: Record<number, string> = { 0: '明文密钥', 0x0200: 'BEK', 0x0800: '恢复密码', 0x2000: '密码', 0x0100: 'TPM' };
  return labels[value] ?? `0x${value.toString(16).padStart(4, '0')}`;
}
async function refreshMetadata() {
  const captured = generation;
  try {
    const result = await bitlockerMetadata({ imageId: props.imageId, partitionId: props.partition.id });
    if (current(captured)) metadata.value = result;
  } catch { /* Status remains available even when metadata is damaged. */ }
}
async function submitKey(credential: BitlockerKeyCredential) {
  if (job.value) return;
  const captured = generation;
  const ok = await startBitlockerKeyUnlock(props.imageId, props.partition.id, credential);
  if (ok && current(captured)) { popoverOpen.value = false; await refreshStatus(); }
}

function focusActiveInput() {
  if (mode.value === 'password') {
    passwordEl.value?.focus();
  } else if (mode.value === 'recovery') {
    recoveryEl.value?.focus();
  }
}

async function submitPassword() {
  if (!canSubmitPassword.value || job.value) return;
  const captured = generation;
  const password = passwordInput.value;
  const ok = await startBitlockerPasswordUnlock(props.imageId, props.partition.id, password);
  if (ok && current(captured)) {
    passwordInput.value = '';
    popoverOpen.value = false;
    await refreshStatus();
  }
}

async function submitRecovery() {
  recoveryTouched.value = true;
  if (!canSubmitRecovery.value || job.value) return;
  const captured = generation;
  const recovery = recoveryCheck.value.normalized;
  const ok = await startBitlockerRecoveryUnlock(props.imageId, props.partition.id, recovery);
  if (ok && current(captured)) {
    recoveryInput.value = '';
    recoveryTouched.value = false;
    popoverOpen.value = false;
    await refreshStatus();
  }
}

async function pickMemoryFile() {
  const captured = generation;
  const selected = await openDialog({
    multiple: false,
    title: '选择内存镜像',
    filters: [
      { name: 'Memory dump', extensions: ['raw', 'mem', 'bin', 'dmp', 'vmem'] },
      { name: '所有文件', extensions: ['*'] },
    ],
  });
  if (!current(captured) || !selected || typeof selected !== 'string') return;
  memoryPath.value = selected;
  candidates.value = null;
}

async function startScan() {
  if (!memoryPath.value || scanJob.value) return;
  const captured = generation;
  const result = await startBitlockerMemoryScan(
    props.imageId,
    props.partition.id,
    memoryPath.value,
  );
  if (result && current(captured)) {
    candidates.value = result.candidates;
  }
}

async function unlockWithCandidate(candidate: BitlockerVmkCandidate) {
  if (job.value || candidate.already_tried) return;
  const captured = generation;
  const ok = await startBitlockerVmkUnlock(
    props.imageId,
    props.partition.id,
    candidate.vmk_hex,
  );
  if (!current(captured)) return;
  if (ok) {
    popoverOpen.value = false;
    candidates.value = null;
    memoryPath.value = '';
    await refreshStatus();
  } else {
    // Backend marked this VMK as already-tried; reflect that locally so the
    // row dims instead of remaining clickable.
    candidate.already_tried = true;
  }
}

async function exportDecryptedImage() {
  if (locked.value || exportJob.value) return;
  await startBitlockerDecryptedImageExport({
    imageId: props.imageId,
    partitionId: props.partition.id,
    defaultFileName: `bitlocker-part${props.partition.id}-decrypted-full.dd`,
  });
}

function resetMemory() {
  candidates.value = null;
}

async function onCancelUnlock() {
  await cancelBitlockerUnlock(props.imageId, props.partition.id);
}

async function onCancelScan() {
  await cancelBitlockerMemoryScan(props.imageId, props.partition.id);
}

watch(
  () => [props.imageId, props.partition.id] as const,
  () => {
    generation += 1;
    locked.value = true;
    encryption.value = null;
    candidates.value = null;
    memoryPath.value = '';
    clearCredentials();
    metadata.value = null;
    void refreshMetadata();
    void refreshStatus();
  },
  { immediate: true, flush: 'sync' },
);

// Re-fetch lock state when an unlock job for this partition wraps up so the
// badge / mount button reflect reality.
watch(
  () => job.value,
  (next, prev) => {
    if (prev && !next) {
      refreshStatus();
    }
  },
);

</script>

<style scoped>
.bde-control {
  position: relative;
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  white-space: nowrap;
}

.bde-status {
  letter-spacing: 0.06em;
}

.bde-status.is-locked {
  border-color: var(--border-color);
}

.bde-chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  height: 26px;
  padding: 0 10px;
  background: var(--sidebar-active-bg);
  border: 1px solid color-mix(in srgb, var(--primary-color) 55%, var(--border-color));
  color: var(--primary-color);
  border-radius: var(--radius-md);
  font-size: 12px;
  white-space: nowrap;
  cursor: pointer;
  transition: background var(--transition-fast);
}

.bde-chip:hover {
  background: var(--sev-info-bg-hover);
}

.bde-chip-dot {
  flex-shrink: 0;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--primary-color);
  animation: bde-pulse 1.4s ease-in-out infinite;
}

@keyframes bde-pulse {
  0%, 100% { opacity: 0.4; }
  50% { opacity: 1; }
}

.bde-chip-pct {
  font-family: var(--font-mono);
  font-size: 11px;
}

.bde-popover {
  position: fixed;
  top: var(--bde-pop-top, -10000px);
  left: var(--bde-pop-left, -10000px);
  z-index: 1000;
  width: 420px;
  max-width: min(420px, calc(100vw - 16px));
  max-height: calc(100vh - 16px);
  overflow-y: auto;
  background: var(--surface-secondary);
  border: 1px solid var(--border-color);
  border-radius: var(--radius-lg);
  box-shadow: var(--modal-shadow);
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  color: var(--text-primary);
}

.bde-popover-head {
  display: flex;
  align-items: center;
  gap: 8px;
}

.bde-popover-icon {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border-radius: var(--radius-md);
  background: var(--sev-info-bg);
  color: var(--primary-color);
}

.bde-popover-title {
  font-size: 13px;
  font-weight: 600;
}

.bde-tabs {
  display: flex;
}

.bde-tab {
  flex: 1;
}

.bde-tab.is-active {
  background: var(--surface-secondary);
  color: var(--primary-color);
  box-shadow: var(--shadow-sm), 0 0 0 1px var(--border-color);
}

</style>
