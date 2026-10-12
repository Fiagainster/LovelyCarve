import { beforeEach, describe, expect, it, vi } from 'vitest';

// search.ts transitively imports the Tauri IPC boundary; mock it so the typed
// wrappers can be unit-tested without a Tauri runtime (same pattern as
// useFileSession.test.ts).
vi.mock('./client', () => ({
  invokeCmd: vi.fn(),
}));

vi.mock('./events', () => ({
  onEvent: vi.fn(),
}));

import { invokeCmd } from './client';
import {
  globalSearchExport,
  globalSearchHitsPage,
  globalSearchHistoryDelete,
  globalSearchHistoryList,
  globalSearchStart,
  onGlobalSearchProgress,
} from './search';

const options = {
  keys: ['password'],
  regexes: [],
  codepages: ['UTF-8', 'GBK', 'UTF-16LE'],
  match_case: false,
  search_deleted: true,
  search_empty_space: false,
  calc_hash: false,
  scope: { partitions: [], tags: [], tree_nodes: [] },
  per_file_cap: 100,
  stop_after_first: false,
  global_cap: 200_000,
};

describe('global search IPC wrappers', () => {
  beforeEach(() => {
    vi.mocked(invokeCmd).mockReset();
  });

  it('routes start / page / history / delete / export through the typed boundary', async () => {
    vi.mocked(invokeCmd)
      .mockResolvedValueOnce({})
      .mockResolvedValueOnce({ search_id: 'search-1', items: [], total: 0 })
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce(true)
      .mockResolvedValueOnce({ output_path: 'out.csv', record_count: 3 });

    await globalSearchStart({ imageId: 'image-1', searchId: 'search-1', options });
    await globalSearchHitsPage({
      imageId: 'image-1',
      searchId: 'search-1',
      skip: 0,
      limit: 200,
      keyword: 'admin',
      distinct: 'file',
      afterSeq: 12,
    });
    await globalSearchHistoryList({ imageId: 'image-1', limit: 50 });
    await globalSearchHistoryDelete({ imageId: 'image-1', searchId: 'search-1' });
    await globalSearchExport({
      imageId: 'image-1',
      searchId: 'search-1',
      destination: 'out.csv',
      distinct: 'file',
    });

    expect(invokeCmd).toHaveBeenNthCalledWith(1, 'global_search_start', {
      imageId: 'image-1',
      searchId: 'search-1',
      options,
    });
    expect(invokeCmd).toHaveBeenNthCalledWith(2, 'global_search_hits_page', {
      imageId: 'image-1',
      searchId: 'search-1',
      skip: 0,
      limit: 200,
      keyword: 'admin',
      distinct: 'file',
      afterSeq: 12,
    });
    expect(invokeCmd).toHaveBeenNthCalledWith(3, 'global_search_history_list', {
      imageId: 'image-1',
      limit: 50,
    });
    expect(invokeCmd).toHaveBeenNthCalledWith(4, 'global_search_history_delete', {
      imageId: 'image-1',
      searchId: 'search-1',
    });
    expect(invokeCmd).toHaveBeenNthCalledWith(5, 'global_search_export', {
      imageId: 'image-1',
      searchId: 'search-1',
      destination: 'out.csv',
      distinct: 'file',
    });
  });

  it('subscribes progress on the global-search-progress event', async () => {
    const { onEvent } = await import('./events');
    vi.mocked(onEvent).mockResolvedValue(() => undefined);
    const handler = () => undefined;
    await onGlobalSearchProgress(handler);
    expect(onEvent).toHaveBeenCalledWith('global-search-progress', handler);
  });
});
