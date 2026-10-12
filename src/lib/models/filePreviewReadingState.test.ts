import { describe, expect, it } from 'vitest';
import { parseFilePreviewReadingState, type FilePreviewReadingState } from './filePreviewReadingState';

const context: FilePreviewReadingState = { imageId: 'image', nodeId: 'file', tab: 'hex', searchQuery: '', encoding: '', page: 1, pageSize: 2048, currentMatch: -1, scrollTop: 0, scrollLeft: 0, codeScrollTop: 0, codeScrollLeft: 0 };
describe('preview context transport validation', () => {
  it('round trips a serializable context', () => {
    expect(parseFilePreviewReadingState(JSON.parse(JSON.stringify(context)))).toEqual(context);
  });
  it.each([{ page: 0 }, { page: NaN }, { pageSize: '2048' }, { tab: 'unknown' }, { scrollTop: Infinity }, { scrollLeft: -1 }, { currentMatch: -2 }])('rejects invalid context %j', invalid => {
    expect(parseFilePreviewReadingState({ ...context, ...invalid })).toBeUndefined();
  });
});
