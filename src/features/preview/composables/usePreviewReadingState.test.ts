// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from 'vitest';
import { effectScope, nextTick, ref } from 'vue';
import { usePreviewReadingState } from './usePreviewReadingState';
import type { FilePreviewReadingState } from '../../../lib/models/filePreviewReadingState';

const scopes: ReturnType<typeof effectScope>[] = [];
const context: FilePreviewReadingState = { imageId: 'image', nodeId: 'file', tab: 'hex', searchQuery: 'needle', encoding: 'utf-8', page: 3, pageSize: 1024, currentMatch: 1, scrollTop: 240, scrollLeft: 80, codeScrollTop: 100, codeScrollLeft: 30 };
async function settle() { for (let index = 0; index < 8; index++) await nextTick(); }
afterEach(() => { for (const scope of scopes.splice(0)) scope.stop(); });
function setup() {
  const scope = effectScope(); scopes.push(scope);
  const body = ref<HTMLElement | null>(document.createElement('div'));
  const nodeId = ref('file'), ready = ref(false), initial = ref<FilePreviewReadingState | undefined>(context);
  const tab = ref<FilePreviewReadingState['tab']>('raw'), searchQuery = ref(''), encoding = ref('');
  const page = ref(1), pageSize = ref(2048), currentMatch = ref(-1), matchCount = ref(2), totalPages = ref(8), codeView = ref<unknown>(null);
  const restoreMatch = vi.fn(index => { currentMatch.value = index; });
  const reading = scope.run(() => usePreviewReadingState({
    imageId: () => 'image', nodeId: () => nodeId.value, initial: () => initial.value, body,
    tab, searchQuery, encoding, page, pageSize, currentMatch, matchCount, totalPages,
    ready: () => ready.value, codeView: () => codeView.value, restoreMatch,
  }))!;
  return { reading, nodeId, ready, initial, body, tab, searchQuery, page, pageSize, matchCount, codeView, restoreMatch, scope };
}
describe('preview reading context', () => {
  it('waits for asynchronous viewer matches before restoring the current hit', async () => {
    const state = setup(); state.matchCount.value = 0; state.ready.value = true;
    await settle();
    expect(state.restoreMatch).not.toHaveBeenCalled();
    state.matchCount.value = 2; await settle();
    expect(state.restoreMatch).toHaveBeenCalledWith(1);
    expect(state.body.value!.scrollTop).toBe(240);
  });
  it('restores search, pagination and both scroll positions only after content is ready', async () => {
    const state = setup();
    const code = document.createElement('div'); code.className = 'cm-scroller'; state.body.value!.append(code);
    await settle();
    expect(state.reading.getReadingState()).toEqual(context);
    expect(state.restoreMatch).not.toHaveBeenCalled();
    state.ready.value = true; await settle();
    expect(state.restoreMatch).toHaveBeenCalledWith(1);
    expect(state.reading.getReadingState()).toEqual(context);
    expect([state.body.value!.scrollTop, state.body.value!.scrollLeft, code.scrollTop, code.scrollLeft]).toEqual([240, 80, 100, 30]);
  });
  it('drops late restoration after switching to another file', async () => {
    const state = setup(); await settle();
    state.ready.value = true; state.nodeId.value = 'other'; state.initial.value = undefined;
    await settle();
    expect(state.restoreMatch).not.toHaveBeenCalled();
    expect(state.body.value!.scrollTop).toBe(0);
    expect(state.reading.getReadingState().nodeId).toBe('other');
  });
  it('honors a user view change during loading', async () => {
    const state = setup(); await settle();
    state.tab.value = 'detail'; await settle();
    state.ready.value = true; await settle();
    expect(state.restoreMatch).not.toHaveBeenCalled();
    expect(state.reading.getReadingState().tab).toBe('detail');
    state.scope.stop();
  });
});
