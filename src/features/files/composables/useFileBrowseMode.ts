import { onScopeDispose, ref } from 'vue';
import { FLAT_KEYWORD_DEBOUNCE_MS, FLAT_SERVER_FILTER_COLUMNS } from '../model/flatQuery';

/** Browse-mode and keyword intent; querying is delegated to the panel. */
export function useFileBrowseMode(options: {
  filters: () => Record<string, string>;
  effectiveFilters: () => Record<string, string>;
  rebasePath: () => void;
  loadCurrent: () => Promise<void>;
}) {
  const deepsearch = ref(false);
  // 搜索框文本（即时）与去抖后实际发给后端的关键词。
  const searchKeyword = ref('');
  const appliedKeyword = ref('');
  let keywordDebounceTimer: number | null = null;
  // 切走面板时自动退出全显；切回
  // 来按逐级浏览重载一次。
  let reloadOnActivate = false;

  /** 全显开关的唯一入口：切换即清勾选并重载。 */
  function showAllFiles(status: 0 | 1) {
    const next = status === 1;
    if (deepsearch.value === next) return;
    deepsearch.value = next;
    if (next) {
      // 全显只能按服务端支持的列过滤：其它列上残留的漏斗条件在这里既不生效
      // 也看不见，属于「隐形过滤」，进入时一律清掉。
      for (const key of Object.keys(options.filters())) {
        if (!FLAT_SERVER_FILTER_COLUMNS.has(key) && options.filters()[key]) {
          options.filters()[key] = '';
          options.effectiveFilters()[key] = '';
        }
      }
    } else {
      // 回到逐级浏览时锚定路径筛选改写到当前位置：原值可能指向当前层之外，
      // 留着会把这一层过滤成空。
      options.rebasePath();
    }
    options.loadCurrent();
  }

  function toggleDeepsearch() {
    showAllFiles(deepsearch.value ? 0 : 1);
  }

  // 搜索框：有关键词自动进入全显，清空自动退出。
  // 去抖后才落到 appliedKeyword，避免每击键打一次后端。
  function onSearchKeywordInput() {
    if (keywordDebounceTimer !== null) window.clearTimeout(keywordDebounceTimer);
    keywordDebounceTimer = window.setTimeout(applySearchKeyword, FLAT_KEYWORD_DEBOUNCE_MS);
  }

  function applySearchKeyword() {
    keywordDebounceTimer = null;
    const keyword = searchKeyword.value.trim();
    if (keyword === appliedKeyword.value) return;
    appliedKeyword.value = keyword;
    // 已在全显中的关键词变化由请求签名 watch 重载；这里只管开关。
    showAllFiles(keyword ? 1 : 0);
  }

  function clearSearchKeyword() {
    if (keywordDebounceTimer !== null) {
      window.clearTimeout(keywordDebounceTimer);
      keywordDebounceTimer = null;
    }
    searchKeyword.value = '';
    applySearchKeyword();
  }

  /** 跳转离开时复位，不在此刻重载（面板已不可见）。 */
  function closeDeepSearch() {
    if (keywordDebounceTimer !== null) {
      window.clearTimeout(keywordDebounceTimer);
      keywordDebounceTimer = null;
    }
    searchKeyword.value = '';
    appliedKeyword.value = '';
    if (!deepsearch.value) return;
    deepsearch.value = false;
    options.rebasePath();
    reloadOnActivate = true;
  }

  onScopeDispose(() => {
    if (keywordDebounceTimer !== null) window.clearTimeout(keywordDebounceTimer);
  });

  function reset() {
    if (keywordDebounceTimer !== null) window.clearTimeout(keywordDebounceTimer);
    keywordDebounceTimer = null;
    deepsearch.value = false;
    searchKeyword.value = '';
    appliedKeyword.value = '';
    reloadOnActivate = false;
  }
  function consumeReload() {
    const reload = reloadOnActivate;
    reloadOnActivate = false;
    return reload;
  }
  return { deepsearch, searchKeyword, appliedKeyword, toggleDeepsearch, onSearchKeywordInput,
    clearSearchKeyword, closeDeepSearch, reset, consumeReload };
}
