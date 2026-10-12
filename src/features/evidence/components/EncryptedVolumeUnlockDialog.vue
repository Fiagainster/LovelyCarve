<template>
  <Teleport to="body">
    <Transition name="ui-modal">
      <div v-if="open" class="ui-modal-overlay" @click.self="emit('close')">
        <div
          ref="dialogEl"
          class="ui-modal unlock-modal"
          role="dialog"
          aria-modal="true"
          aria-labelledby="encrypted-unlock-title"
          tabindex="-1"
          @keydown.esc.stop="emit('close')"
        >
          <header class="ui-modal-header">
            <span class="ui-modal-icon"><LockKeyhole :size="18" /></span>
            <div class="ui-modal-heading">
              <h2 id="encrypted-unlock-title" class="ui-modal-title">{{ partitionId === null ? '解锁加密容器' : `解锁分区 ${partitionId}` }}</h2>
            </div>
            <button class="ui-icon-btn" type="button" title="关闭" aria-label="关闭" @click="emit('close')"><X :size="16" /></button>
          </header>
          <form class="unlock-form" @submit.prevent="unlock">
            <div class="ui-modal-body unlock-body">
              <fieldset :class="{ fixed: partitionId !== null }">
                <label>加密格式
                  <UiSelect v-model="scheme" :options="SCHEME_OPTIONS" />
                </label>
                <label v-if="partitionId === null">加密区域
                  <UiSelect v-model="region" :options="regionOptions" />
                </label>
                <label class="wide">密码<input v-model="password" class="ui-input" type="password" autocomplete="off" required maxlength="4096" /></label>
                <template v-if="scheme === 'veracrypt'">
                  <label>PIM（0 为默认）<input v-model.number="pim" class="ui-input mono" type="number" min="0" max="1000" step="1" required /></label>
                  <label class="ui-check check"><input v-model="hidden" type="checkbox" />打开隐藏卷</label>
                </template>
              </fieldset>
            </div>
            <footer class="ui-modal-footer">
              <button class="ui-btn" type="button" @click="emit('close')">取消</button>
              <button class="ui-btn primary" type="submit" :disabled="!password">开始后台解锁</button>
            </footer>
          </form>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { LockKeyhole, X } from 'lucide-vue-next';
import type { ImageInfo } from '../../../lib/ipc/image';
import { useDialogFocus } from '../../../composables/useDialogFocus';
import { startEncryptedVolumeUnlock } from '../composables/useEncryptedVolumeUnlocks';
import UiSelect, { type UiSelectOption } from '../../../components/ui/UiSelect.vue';

const props = defineProps<{
  open: boolean;
  info: ImageInfo;
  /** 从分区行打开时预选的分区；从卷卡标题打开为 null（整个镜像）。 */
  partitionId: number | null;
}>();
const emit = defineEmits<{ close: [] }>();
const SCHEME_OPTIONS: UiSelectOption<'luks' | 'veracrypt'>[] = [
  { value: 'luks', label: 'LUKS1 / LUKS2' },
  { value: 'veracrypt', label: 'VeraCrypt' },
];
const scheme = ref<'luks' | 'veracrypt'>('luks');
const region = ref('whole');
const regionOptions = computed<UiSelectOption<string>[]>(() => [
  { value: 'whole', label: '整个镜像 / 容器' },
  ...props.info.partitions.map(partition => ({ value: String(partition.id), label: `分区 ${partition.id} · ${partition.label || partition.kind}` })),
]);
const password = ref('');
const pim = ref(0);
const hidden = ref(false);
const dialogEl = ref<HTMLElement | null>(null);

useDialogFocus(() => props.open, dialogEl);

// 每次打开都从触发它的位置开始：分区行预选该分区（LUKS 分区同时选 LUKS），
// 卷卡标题选整个镜像。关闭即清空密码；后台任务不持有弹窗。
watch(() => props.open, isOpen => {
  if (!isOpen) {
    password.value = '';
    return;
  }
  const partition = props.info.partitions.find(candidate => candidate.id === props.partitionId);
  if (partition?.kind === 'luks') scheme.value = 'luks';
  region.value = partition ? String(partition.id) : 'whole';
}, { immediate: true });

function unlock() {
  if (!password.value) return;
  const options = {
    scheme: scheme.value, password: password.value,
    ...(props.partitionId !== null ? { partitionId: props.partitionId } : region.value === 'whole' ? {} : { partitionId: Number(region.value) }),
    ...(scheme.value === 'veracrypt' ? { pim: pim.value, hidden: hidden.value } : {}),
  };
  password.value = '';
  if (startEncryptedVolumeUnlock(props.info, options)) emit('close');
}

onBeforeUnmount(() => {
  password.value = '';
  // The backend job remains visible in the task center when changing views.
});
</script>

<style scoped>
.unlock-modal { width: min(520px, calc(100vw - 32px)); }
.unlock-form { display: flex; flex: 1; flex-direction: column; min-height: 0; }
.unlock-body { display: grid; gap: 16px; }
fieldset { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; min-width: 0; margin: 0; padding: 0; border: 0; }
fieldset.fixed { grid-template-columns: minmax(0, 1fr); }
label { display: grid; gap: 6px; min-width: 0; color: var(--text-primary); font-size: 12px; font-weight: 500; }
label .ui-select { width: 100%; }
.wide { grid-column: 1 / -1; }
.check { display: flex; align-self: end; height: 30px; }
@media (max-width: 520px) {
  fieldset { grid-template-columns: minmax(0, 1fr); }
}
</style>
