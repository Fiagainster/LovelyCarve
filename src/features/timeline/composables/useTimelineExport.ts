import { onScopeDispose, ref, watch } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { timelineExport, type TimelineQuery } from '../../../lib/ipc/timeline';
import { notifyError, notifyExportSuccess } from '../../../composables/useToast';
export function useTimelineExport(options: { imageId: () => string; buildQuery: () => TimelineQuery; total: () => number }) {
  const exporting = ref(false);
  let generation = 0; let disposed = false;
  async function exportRows() {
    if (disposed || exporting.value || options.total() === 0) return;
    const imageId = options.imageId(); const query = options.buildQuery(); const captured = ++generation;
    exporting.value = true;
    try {
      const destination = await save({ defaultPath: `时间线_${imageId}.csv`, filters: [{ name: 'CSV', extensions: ['csv'] }] });
      if (!destination || disposed || captured !== generation) return;
      const result = await timelineExport({ imageId, query, destination });
      if (!disposed) notifyExportSuccess(result.output_path, `已导出 ${result.record_count.toLocaleString()} 条记录`);
    } catch (error) { if (!disposed) notifyError('导出时间线失败', String(error)); }
    finally { if (!disposed && captured === generation) exporting.value = false; }
  }
  watch(options.imageId, () => { generation++; exporting.value = false; }, { flush: 'sync' });
  onScopeDispose(() => { disposed = true; generation++; });
  return { exporting, exportRows };
}
