import { describe, expect, it } from 'vitest';
import { shouldShowDirectoryWithEmptyFilter } from './emptyDirectoryFilter';

describe('shouldShowDirectoryWithEmptyFilter', () => {
  it('keeps unknown directories hidden until classification finishes', () => {
    expect(shouldShowDirectoryWithEmptyFilter(undefined, undefined)).toBe(false);
    expect(shouldShowDirectoryWithEmptyFilter(undefined, null)).toBe(false);
  });

  it('uses the recursive snapshot result when available', () => {
    expect(shouldShowDirectoryWithEmptyFilter(true, 3)).toBe(false);
    expect(shouldShowDirectoryWithEmptyFilter(false, 0)).toBe(true);
  });

  it('falls back to the direct child count without a snapshot result', () => {
    expect(shouldShowDirectoryWithEmptyFilter(undefined, 0)).toBe(false);
    expect(shouldShowDirectoryWithEmptyFilter(undefined, 1)).toBe(true);
  });
});
