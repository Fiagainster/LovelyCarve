// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref } from 'vue';
import { usePreviewSearch } from './usePreviewSearch';
import { renderMarkdownPreview } from './useMarkdown';
import type { DetailTab, PreviewCategory } from '../model/preview';

const scopes: ReturnType<typeof effectScope>[] = [];
afterEach(() => { for (const scope of scopes.splice(0)) scope.stop(); });
async function settle() { for (let index = 0; index < 4; index++) await nextTick(); }
function setup(tab: DetailTab = 'text', kind: PreviewCategory = 'markdown') {
  const scope = effectScope(); scopes.push(scope);
  const searchQuery = ref(''), activeTab = ref<DetailTab>(tab), category = ref<PreviewCategory>(kind);
  const decodedText = ref(''), renderedMarkdown = ref(renderMarkdownPreview(''));
  const page = ref(1), pageSize = ref(32), currentHexStart = ref(-1), hexMatch = ref({ count: 2, starts: [0, 48] });
  const search = scope.run(() => usePreviewSearch({ searchQuery, activeTab, category, decodedText, renderedMarkdown, page, pageSize, currentHexStart, hexMatch }))!;
  return { search, searchQuery, activeTab, category, decodedText, renderedMarkdown, page, pageSize, currentHexStart, scope };
}

describe('preview search and focus', () => {
  it('highlights literal Markdown matches only in text nodes and never treats the query as HTML', async () => {
    const state = setup('raw'); const root = document.createElement('div'); state.search.mdEl.value = root;
    state.decodedText.value = '**a.b** a.b axb <img src=x onerror=alert(1)>';
    state.renderedMarkdown.value = renderMarkdownPreview(state.decodedText.value);
    state.searchQuery.value = 'a.b'; await settle();
    expect(state.search.matchCount.value).toBe(2);
    expect([...root.querySelectorAll('mark')].map(mark => mark.textContent)).toEqual(['a.b', 'a.b']);
    expect(root.querySelector('img')).toBeNull();
    state.searchQuery.value = '<img'; await settle();
    expect(root.querySelector('mark')?.textContent).toBe('<img'); expect(root.querySelector('img')).toBeNull();
  });
  it('cycles text hits in both directions and resets current marks when the query changes', async () => {
    const state = setup(); const root = document.createElement('pre'); root.innerHTML = '<mark>one</mark> <mark>one</mark>';
    root.querySelectorAll('mark').forEach(mark => { mark.scrollIntoView = vi.fn(); }); state.search.textEl.value = root;
    state.decodedText.value = 'one one'; state.searchQuery.value = 'one'; await settle();
    state.search.gotoMatch(-1); expect(state.search.currentMatch.value).toBe(1);
    expect(root.querySelectorAll('mark')[1].classList.contains('current')).toBe(true);
    state.search.gotoMatch(1); expect(state.search.currentMatch.value).toBe(0);
    state.searchQuery.value = 'different'; await settle();
    expect(state.search.currentMatch.value).toBe(-1); expect(root.querySelector('mark.current')).toBeNull();
  });
  it('focuses a hex hit on the right page and cancels deferred scrolling after disposal', async () => {
    const state = setup('hex'); const root = document.createElement('div');
    const scroll = vi.fn();
    for (let index = 0; index < 3; index++) { const row = document.createElement('div'); row.className = 'hex-row'; row.scrollIntoView = scroll; root.append(row); }
    state.search.hexEl.value = root; state.searchQuery.value = 'x'; await settle();
    state.search.gotoMatch(-1); expect(state.page.value).toBe(2); expect(state.currentHexStart.value).toBe(48);
    state.scope.stop(); await settle(); expect(scroll).not.toHaveBeenCalled();
  });
  it('delegates raw code/CSV focus to their own viewers and hides counts for media', async () => {
    const state = setup('raw', 'code'); state.decodedText.value = 'one one'; state.searchQuery.value = 'one'; await settle();
    const code = vi.fn(), csv = vi.fn(); state.search.codeViewerRef.value = { focusMatch: code }; state.search.csvViewerRef.value = { focusMatch: csv };
    state.search.gotoMatch(1); expect(code).toHaveBeenCalledWith(0);
    state.search.restoreMatch(1); expect(code).toHaveBeenLastCalledWith(1, false);
    expect(state.search.currentMatch.value).toBe(1);
    state.category.value = 'csv'; state.search.csvMatchCount.value = 3; state.search.resetMatchNav();
    state.search.gotoMatch(-1); expect(csv).toHaveBeenLastCalledWith(2);
    state.category.value = 'image'; expect(state.search.matchCount.value).toBe(0);
  });
  it('preserves IME input and consumes search Escape before the panel can close', async () => {
    const state = setup(); state.searchQuery.value = 'one'; state.decodedText.value = 'one'; await settle();
    state.search.onSearchKeydown(new KeyboardEvent('keydown', { key: 'Enter', isComposing: true }));
    expect(state.search.currentMatch.value).toBe(-1);
    const escape = new KeyboardEvent('keydown', { key: 'Escape' }); const stop = vi.spyOn(escape, 'stopPropagation');
    state.search.onSearchKeydown(escape); expect(state.searchQuery.value).toBe(''); expect(stop).toHaveBeenCalledOnce();
  });
});
