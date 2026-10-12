import { computed, onActivated, onMounted, onScopeDispose, ref, shallowRef, watch } from 'vue';
import { trackedTimelineStart as timelineStart } from '../../tasks/api';
import { timelineStatus, onTimelineProgress, type TimelineStatusInfo } from '../../../lib/ipc/timeline';
import { taskCancel } from '../../../lib/ipc/taskRuntime';
import { notifyError, notifyInfo } from '../../../composables/useToast';
export function useTimelineBuild(options: { imageId: () => string; onBuilt: () => void }) {
  let generation = 0;
  let statusGeneration = 0;
  let disposed = false;
  let unlisten: (() => void) | null = null;
  const statusInfo = shallowRef<TimelineStatusInfo | null>(null);
  const statusError = ref('');
  const building = ref(false);
  const progress = ref<{ done: number; total: number; rows: number } | null>(null);

  const statusChipClass = computed(() => {
    switch (statusInfo.value?.status) {
      case 'ready':
        return 'success';
      case 'running':
        return 'info';
      case 'stale':
        return 'warn';
      default:
        return '';
    }
  });

  const buildButtonLabel = computed(() => {
    if (building.value || statusInfo.value?.status === 'running') return '取消构建';
    switch (statusInfo.value?.status) {
      case 'buildRequired':
        return '创建时间线';
      case 'stale':
        return '更新时间线';
      default:
        return '重建时间线';
    }
  });

  async function refreshStatus(): Promise<TimelineStatusInfo | null> {
    if (disposed) return null;
    const imageId = options.imageId();
    const captured = generation;
    const request = ++statusGeneration;
    try {
      const info = await timelineStatus({ imageId });
      if (captured !== generation || request !== statusGeneration || disposed) return null;
      statusInfo.value = info;
      statusError.value = '';
      return info;
    } catch (error) {
      if (captured === generation && request === statusGeneration && !disposed) {
        notifyError('读取时间线状态失败', String(error));
        statusInfo.value = null;
        statusError.value = String(error);
      }
      return null;
    }
  }

  async function startBuild() {
    if (disposed || building.value) return;
    const imageId = options.imageId();
    const captured = generation;
    building.value = true;
    progress.value = { done: 0, total: 0, rows: 0 };
    try {
      const summary = await timelineStart({ imageId });
      if (captured !== generation || disposed) return;
      if (summary.status === 'cancelled') {
        notifyInfo('时间线构建已取消', '已落库的部分投影保留，可随时重建。');
      } else {
        notifyInfo(
          '时间线构建完成',
          `文件 ${summary.file_rows.toLocaleString()} 条、插件事件 ${summary.event_rows.toLocaleString()} 条` +
          (summary.skipped_invalid_time > 0
            ? `（跳过 ${summary.skipped_invalid_time.toLocaleString()} 条无有效时间戳的记录）`
            : ''),
        );
      }
    } catch (error) {
      if (captured === generation && !disposed) notifyError('时间线构建失败', String(error));
    } finally {
      if (captured === generation && !disposed) {
        building.value = false;
        progress.value = null;
        await refreshStatus();
        if (captured === generation && !disposed) options.onBuilt();
      }
    }
  }

  async function buildButtonClicked() {
    if (disposed) return;
    if (building.value || statusInfo.value?.status === 'running') {
      const captured = generation;
      try {
        await taskCancel({ taskId: `timeline-build-${options.imageId()}` });
      } catch (error) {
        if (captured === generation && !disposed) notifyError('取消时间线构建失败', String(error));
      }
      return;
    }
    await startBuild();
  }


  function reset() {
    generation++; statusGeneration++; statusInfo.value = null; statusError.value = ''; building.value = false; progress.value = null;
  }
  watch(options.imageId, reset, { flush: 'sync' });
  onMounted(() => {
    void onTimelineProgress(event => {
      if (disposed || event.payload.image_id !== options.imageId()) return;
      progress.value = { done: event.payload.done, total: event.payload.total, rows: event.payload.rows };
    }).then(cleanup => { if (disposed) cleanup(); else unlisten = cleanup; })
      .catch(error => { if (!disposed) notifyError('订阅时间线进度失败', String(error)); });
  });
  onActivated(() => { if (statusInfo.value) void refreshStatus(); });
  onScopeDispose(() => { disposed = true; generation++; statusGeneration++; unlisten?.(); unlisten = null; });
  return { statusInfo, statusError, building, progress, statusChipClass, buildButtonLabel, refreshStatus, buildButtonClicked };
}
