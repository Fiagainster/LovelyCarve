import { describe, expect, it } from 'vitest';

import {
  UNIT_LABELS,
  bucketRange,
  bucketShortLabel,
  dataTypeLabel,
  drillDown,
  formatEpoch,
  formatRange,
  statusLabel,
  zoomUp,
} from './timelineView';

describe('timelineView granularity ladder', () => {
  it('drills down one step and clamps at second', () => {
    expect(drillDown('year')).toBe('month');
    expect(drillDown('hour')).toBe('minute');
    expect(drillDown('second')).toBe('second');
  });

  it('zooms up one step and clamps at year', () => {
    expect(zoomUp('month')).toBe('year');
    expect(zoomUp('year')).toBe('year');
  });

  it('labels every unit in Chinese', () => {
    expect(UNIT_LABELS.minute).toBe('分');
  });
});

describe('timelineView bucketRange', () => {
  it('converts a year bucket back to inclusive unix seconds in UTC', () => {
    // 2024 年桶（UTC 聚合）：[2024-01-01 00:00:00, 2024-12-31 23:59:59]。
    expect(bucketRange('2024', 'year', 0)).toEqual({
      start: 1704067200,
      end: 1735689599,
    });
  });

  it('applies the aggregation timezone inversely', () => {
    // UTC+8 下 2024 年桶起点是 UTC 2023-12-31 16:00:00。
    expect(bucketRange('2024', 'year', 8).start).toBe(1704067200 - 8 * 3600);
  });

  it('handles hour buckets whose label contains a space', () => {
    // 2024-01-01 08 时桶（UTC）：[08:00:00, 08:59:59]。
    expect(bucketRange('2024-01-01 08', 'hour', 0)).toEqual({
      start: 1704096000,
      end: 1704099599,
    });
  });

  it('carries month overflow into the next year', () => {
    // 2023-12 月桶的 end = 2024-01-01 00:00:00 UTC - 1。
    expect(bucketRange('2023-12', 'month', 0).end).toBe(1704067199);
  });

  it('round-trips with formatEpoch', () => {
    const { start, end } = bucketRange('2024-05-12 08:30', 'minute', 8);
    expect(formatEpoch(start, 8)).toBe('2024-05-12 08:30:00');
    expect(formatEpoch(end, 8)).toBe('2024-05-12 08:30:59');
  });
});

describe('timelineView formatting', () => {
  it('formats epoch in the aggregation timezone', () => {
    expect(formatEpoch(1704067200, 0)).toBe('2024-01-01 00:00:00');
    expect(formatEpoch(1704067200, 8)).toBe('2024-01-01 08:00:00');
  });

  it('shortens bucket labels per unit for the chart axis', () => {
    expect(bucketShortLabel('2024', 'year')).toBe('2024');
    expect(bucketShortLabel('2024-05', 'month')).toBe('05');
    expect(bucketShortLabel('2024-05-12', 'day')).toBe('12');
    expect(bucketShortLabel('2024-05-12 08', 'hour')).toBe('08时');
    expect(bucketShortLabel('2024-05-12 08:30', 'minute')).toBe('30');
    expect(bucketShortLabel('2024-05-12 08:30:59', 'second')).toBe('59');
  });

  it('renders human range text', () => {
    expect(formatRange(1704067200, 1704067200 + 59, 0)).toBe(
      '2024-01-01 00:00:00 ~ 2024-01-01 00:00:59',
    );
  });

  it('maps status and datatype numbers to Chinese labels', () => {
    expect(statusLabel('buildRequired')).toBe('未创建');
    expect(statusLabel('stale')).toBe('有新数据未处理');
    expect(dataTypeLabel(1)).toBe('文件');
    expect(dataTypeLabel(2)).toBe('插件时间线');
    expect(dataTypeLabel(3)).toBe('NTFS 变更日志');
    expect(dataTypeLabel(9)).toBe('类型 9');
  });
});
