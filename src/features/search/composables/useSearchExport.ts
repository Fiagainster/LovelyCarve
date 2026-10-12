import { onScopeDispose, ref, watch } from 'vue';
import { save } from '@tauri-apps/plugin-dialog';
import { globalSearchExport, type GlobalSearchDistinct, type GlobalSearchHistoryEntry } from '../../../lib/ipc/search';
import { notifyError, notifyExportSuccess } from '../../../composables/useToast';
import { taskDisplayTitle } from '../model/searchResults';
interface SearchExportOptions {
  imageId: () => string; selectedTask: () => GlobalSearchHistoryEntry | null;
  keyword: () => string; distinct: () => GlobalSearchDistinct;
}
export function useSearchExport(options: SearchExportOptions) {
  const exporting = ref(false);
  let generation = 0;
  let disposed = false;
  const current = (captured: number) => !disposed && captured === generation;
  async function exportResults() {
    const task = options.selectedTask();
    if (!task || exporting.value || disposed) return;
    const captured = ++generation;
    const imageId = options.imageId();
    const keyword = options.keyword() || undefined;
    const distinct = options.distinct();
    // The chooser is part of the operation: repeated clicks cannot start two exports.
    exporting.value = true;
    try {
      const destination = await save({
        title: '导出全局搜索结果',
        defaultPath: `全局搜索-${taskDisplayTitle(task.keywords, task.regexes).slice(0, 24)}.csv`,
        filters: [{ name: 'CSV', extensions: ['csv'] }]
      });
      if (!destination || !current(captured)) return;
      const result = await globalSearchExport({ imageId, searchId: task.search_id, destination, keyword, distinct });
      // Once started, host output remains actionable even if the visible selection changes.
      if (!disposed) notifyExportSuccess(result.output_path, `已导出 ${result.record_count.toLocaleString()} 条命中`);
    } catch (error) { if (!disposed) notifyError('导出失败', String(error)); }
    finally { if (current(captured)) exporting.value = false; }
  }
  watch([options.imageId, () => options.selectedTask()?.search_id], () => { generation++; exporting.value = false; }, { flush: 'sync' });
  onScopeDispose(() => { disposed = true; generation++; });
  return { exporting, exportResults };
}
