import { describe, expect, it } from 'vitest';
import { useSizeFilter } from './useSizeFilter';

describe('size filter validation', () => {
  it('preserves the applied query when edited bounds are invalid or reversed', () => {
    const controls = useSizeFilter();
    Object.assign(controls.sizeFilter, { leEnabled: true, leValue: '2', leUnit: 'KB' });
    expect(controls.applySizeFilter()).toBe(true);
    Object.assign(controls.sizeFilter, { geEnabled: true, geValue: '3', geUnit: 'KB' });
    expect(controls.applySizeFilter()).toBe(false);
    expect(controls.sizeFilterErrors.value.ge).toContain('不能大于');
    expect(controls.sizeFilter.appliedLe).toBe(2048);
    expect(controls.sizeFilter.appliedGe).toBeNull();
    expect(controls.appliedSizePredicate()?.(2048)).toBe(true);
    expect(controls.appliedSizePredicate()?.(2049)).toBe(false);
    controls.sizeFilter.geValue = '1';
    expect(controls.sizeFilterErrors.value.ge).toBe('');
    expect(controls.applySizeFilter()).toBe(true);
    expect([controls.sizeFilter.appliedGe, controls.sizeFilter.appliedLe]).toEqual([1024, 2048]);
  });
  it.each(['', '-1', 'invalid', 'Infinity', '1e309', '9007199254740992'])('rejects invalid size %s without activating a partial filter', value => {
    const controls = useSizeFilter();
    Object.assign(controls.sizeFilter, { leEnabled: true, leValue: value, leUnit: 'B', geEnabled: true, geValue: '0' });
    expect(controls.applySizeFilter()).toBe(false);
    expect(controls.sizeFilterErrors.value.le).not.toBe('');
    expect(controls.sizeFilter.active).toBe(false);
    expect(controls.sizeFilter.appliedGe).toBeNull();
  });
  it('requires a positive integer divisor and a smaller nonnegative remainder', () => {
    const controls = useSizeFilter();
    Object.assign(controls.sizeFilter, { modEnabled: true, modValue: '0', modRemainder: '-1' });
    expect(controls.applySizeFilter()).toBe(false);
    expect(controls.sizeFilterErrors.value.mod).not.toBe('');
    expect(controls.sizeFilterErrors.value.remainder).not.toBe('');
    controls.sizeFilter.modValue = '512'; controls.sizeFilter.modRemainder = '512';
    expect(controls.applySizeFilter()).toBe(false);
    controls.sizeFilter.modRemainder = '';
    expect(controls.applySizeFilter()).toBe(true);
    expect(controls.sizeFilter.appliedMod).toEqual({ n: 512, r: 0 });
    expect(controls.appliedSizePredicate()?.(1024)).toBe(true);
    expect(controls.appliedSizePredicate()?.(1025)).toBe(false);
  });
});
