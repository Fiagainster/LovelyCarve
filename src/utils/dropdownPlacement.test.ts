import { describe, expect, it } from 'vitest';
import { DROPDOWN_MAX_HEIGHT, placeDropdown } from './dropdownPlacement';

const viewport = { width: 1000, height: 800 };

describe('placeDropdown', () => {
  it('opens below the trigger, aligned to its left edge', () => {
    expect(placeDropdown({ top: 100, bottom: 130, left: 200 }, { width: 160, height: 120 }, viewport)).toEqual({
      top: 134,
      left: 200,
      maxHeight: DROPDOWN_MAX_HEIGHT,
      above: false,
    });
  });

  it('flips above when the list only fits there', () => {
    expect(placeDropdown({ top: 700, bottom: 730, left: 200 }, { width: 160, height: 200 }, viewport)).toEqual({
      top: 496,
      left: 200,
      maxHeight: DROPDOWN_MAX_HEIGHT,
      above: true,
    });
  });

  it('stays on the roomier side and scrolls when neither side fits', () => {
    expect(placeDropdown({ top: 250, bottom: 280, left: 200 }, { width: 160, height: 900 }, { width: 1000, height: 600 })).toEqual({
      top: 284,
      left: 200,
      maxHeight: 308,
      above: false,
    });
  });

  it('caps a long list at the maximum height', () => {
    const placement = placeDropdown({ top: 10, bottom: 40, left: 0 }, { width: 160, height: 2000 }, viewport);
    expect(placement.maxHeight).toBe(DROPDOWN_MAX_HEIGHT);
    expect(placement.above).toBe(false);
  });

  it('keeps the list inside the viewport horizontally', () => {
    expect(placeDropdown({ top: 100, bottom: 130, left: 900 }, { width: 200, height: 100 }, viewport).left).toBe(792);
    expect(placeDropdown({ top: 100, bottom: 130, left: 900 }, { width: 2000, height: 100 }, viewport).left).toBe(8);
  });
});
