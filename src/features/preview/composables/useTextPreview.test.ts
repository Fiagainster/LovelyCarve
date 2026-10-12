import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { computed, effectScope, ref } from 'vue';
import { useTextPreview } from './useTextPreview';
import type { DetailTab, PreviewCategory } from '../model/preview';

const mocks = vi.hoisted(() => ({ decode: vi.fn() }));
vi.mock('../../../lib/ipc/fs/read', () => ({ fsPreviewText: mocks.decode }));
const scopes: ReturnType<typeof effectScope>[] = [];
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(yes => { resolve = yes; }); return { promise, resolve }; }
const decoded = (text: string) => ({ text, encoding: 'UTF-8', from_bom: false, had_errors: false });
function setup() {
  const scope = effectScope(); scopes.push(scope);
  const image = ref('image-a'), node = ref('file-a'), activeTab = ref<DetailTab>('text'), bytes = ref(new TextEncoder().encode('fallback'));
  const text = scope.run(() => useTextPreview({ imageId: () => image.value, nodeId: () => node.value, nodeKind: () => 'file', activeTab,
    category: computed<PreviewCategory>(() => 'code'), bytes, searchQuery: ref(''), previewCap: 1024 * 1024 }))!;
  return { text, image, node, activeTab, scope };
}
beforeEach(() => { vi.clearAllMocks(); mocks.decode.mockResolvedValue(decoded('current')); });
afterEach(() => { for (const scope of scopes.splice(0)) scope.stop(); });

describe('text decoding request owner', () => {
  it('deduplicates decoding for one generation and preserves the UTF-8 fallback on failure', async () => {
    const state = setup(); await Promise.all([state.text.ensureDecodedText(), state.text.ensureDecodedText()]);
    expect(mocks.decode).toHaveBeenCalledOnce(); expect(state.text.decodedText.value).toBe('current');
    state.text.resetTextPreview(); mocks.decode.mockRejectedValueOnce(new Error('decode failed'));
    await state.text.ensureDecodedText(); expect(state.text.decodedText.value).toBe('fallback');
  });
  it('does not publish old text after A→B→A or clear the new decoding gate', async () => {
    const old = deferred<ReturnType<typeof decoded>>(), recent = deferred<ReturnType<typeof decoded>>();
    mocks.decode.mockReturnValueOnce(old.promise).mockReturnValueOnce(Promise.resolve(decoded('b'))).mockReturnValueOnce(recent.promise);
    const state = setup(); const first = state.text.ensureDecodedText(); state.node.value = 'file-b'; await state.text.ensureDecodedText();
    state.node.value = 'file-a'; const third = state.text.ensureDecodedText(); old.resolve(decoded('old')); await first;
    expect(state.text.decoded.value).toBeNull(); await state.text.ensureDecodedText(); expect(mocks.decode).toHaveBeenCalledTimes(3);
    recent.resolve(decoded('fresh')); await third; expect(state.text.decodedText.value).toBe('fresh');
  });
  it('clears manual encoding and old decoded text when evidence changes with the same node id', async () => {
    const state = setup(); await state.text.ensureDecodedText(); state.text.encodingOverride.value = 'utf-8'; state.image.value = 'image-b';
    expect(state.text.encodingOverride.value).toBe(''); expect(state.text.decoded.value).toBeNull();
    await state.text.ensureDecodedText(); expect(mocks.decode).toHaveBeenLastCalledWith({ imageId: 'image-b', nodeId: 'file-a', maxBytes: 1024 * 1024 });
  });
  it('drops successful decoding after final disposal', async () => {
    const old = deferred<ReturnType<typeof decoded>>(); mocks.decode.mockReturnValueOnce(old.promise);
    const state = setup(); const operation = state.text.ensureDecodedText(); state.scope.stop(); old.resolve(decoded('old')); await operation;
    expect(state.text.decoded.value).toBeNull();
  });
});
