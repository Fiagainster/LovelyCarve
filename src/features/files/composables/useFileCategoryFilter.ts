import { computed, ref, type Ref } from 'vue';
import { categoryOfName, type IconCategory } from '../../../utils/fileCategory';
import type { DiskFsNode } from '../model/model';

export function useFileCategoryFilter(entries: Ref<DiskFsNode[]>) {
  const selectedCategories = ref<Set<IconCategory>>(new Set());
  const categoryCache = new Map<string, IconCategory>();

  function categoryOfNode(node: DiskFsNode): IconCategory {
    let category = categoryCache.get(node.name);
    if (category === undefined) {
      category = categoryOfName(node.name);
      categoryCache.set(node.name, category);
    }
    return category;
  }

  function toggleCategory(key: IconCategory) {
    const next = new Set(selectedCategories.value);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    selectedCategories.value = next;
  }

  function clearCategories() {
    if (selectedCategories.value.size > 0) selectedCategories.value = new Set();
  }

  function resetCategoryCache() {
    categoryCache.clear();
  }

  const categoryCounts = computed(() => {
    const counts: Record<string, number> = {};
    let total = 0;
    for (const node of entries.value) {
      if (node.kind !== 'file') continue;
      total += 1;
      const category = categoryOfNode(node);
      counts[category] = (counts[category] ?? 0) + 1;
    }
    return { counts, total };
  });

  return {
    selectedCategories,
    categoryOfNode,
    toggleCategory,
    clearCategories,
    resetCategoryCache,
    categoryCounts,
  };
}
