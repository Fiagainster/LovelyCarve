import { describe, expect, it } from 'vitest';
import { buildHeatmapDays, heatmapLevel, intersectBucketRange } from './timelineHeatmap';
import { bucketRange } from './timelineView';

describe('timeline activity calendar', () => {
  it('includes leap day, fills gaps and assigns counts to the correct date', () => {
    const days = buildHeatmapDays(2024, [{ date_str: '2024-02-29', count: 32 }, { date_str: '2023-12-31', count: 99 }]);
    expect(days).toHaveLength(366);
    expect(days[59]).toEqual({ date: '2024-02-29', count: 32, week: 8, weekday: 3, month: 2, day: 29 });
    expect(days[0]).toEqual({ date: '2024-01-01', count: 0, week: 0, weekday: 0, month: 1, day: 1 });
    expect(days.reduce((sum, day) => sum + day.count, 0)).toBe(32);
    expect(days.at(-1)?.date).toBe('2024-12-31');
  });
  it('keeps Sunday and Monday in adjacent weeks across the year boundary', () => {
    const days = buildHeatmapDays(2023, []);
    expect(days).toHaveLength(365);
    expect(days.slice(0, 2).map(({ week, weekday }) => ({ week, weekday }))).toEqual([{ week: 0, weekday: 6 }, { week: 1, weekday: 0 }]);
    expect(buildHeatmapDays(2012, []).at(-1)?.week).toBe(53);
  });
  it('rejects invalid years and separates empty from low activity days', () => {
    expect(buildHeatmapDays(NaN, [])).toEqual([]);
    expect(buildHeatmapDays(2024.5, [])).toEqual([]);
    expect([0, 1, 25, 26, 50, 51, 75, 76, 100].map(value => heatmapLevel(value, 100))).toEqual([0, 1, 1, 2, 2, 3, 3, 4, 4]);
    expect(heatmapLevel(0, 0)).toBe(0);
  });
  it('does not widen a selected day when a coarse trend bucket is clicked', () => {
    const day = bucketRange('2024-02-29', 'day', 5.5);
    expect(intersectBucketRange('2024', 'year', 5.5, day)).toEqual(day);
    const hour = bucketRange('2024-02-29 23', 'hour', 5.5);
    expect(intersectBucketRange('2024-02-29 23', 'hour', 5.5, day)).toEqual(hour);
    expect(hour.end).toBe(day.end);
  });
});
