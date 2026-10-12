// @vitest-environment happy-dom
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { nextTick } from 'vue';

describe('recent cases', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.resetModules();
  });

  it('keeps eight cases, updates names, and moves reopened cases to the top', async () => {
    const history = (await import('./useRecentCases')).useRecentCases();
    for (let index = 0; index < 10; index++) history.recordCase(`/cases/${index}`, `Case ${index}`);
    history.recordCase('/cases/5', 'Renamed Case');
    expect(history.recentCases.value.map(({ dir, name }) => ({ dir, name }))).toEqual([
      { dir: '/cases/5', name: 'Renamed Case' },
      ...[9, 8, 7, 6, 4, 3, 2].map(index => ({ dir: `/cases/${index}`, name: `Case ${index}` })),
    ]);
    await nextTick();
    expect(JSON.parse(localStorage.getItem('lovelycarve.recentCases')!)).toEqual(history.recentCases.value);
  });

  it('clears only recent history and records subsequent case opens again', async () => {
    const history = (await import('./useRecentCases')).useRecentCases();
    localStorage.setItem('unrelated-key', 'keep');
    history.recordCase('/cases/one', 'Case One');
    history.clearRecentCases();
    await nextTick();
    expect(localStorage.getItem('lovelycarve.recentCases')).toBe('[]');
    expect(localStorage.getItem('unrelated-key')).toBe('keep');
    history.recordCase('/cases/one', 'Case One');
    expect(history.recentCases.value).toHaveLength(1);
  });
});
