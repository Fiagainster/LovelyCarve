import type { TimelineBucket } from '../../../lib/ipc/timeline';
import { bucketRange } from './timelineView';

export interface HeatmapDay {
  date: string;
  count: number;
  week: number;
  weekday: number;
  month: number;
  day: number;
}

/** Monday-first calendar, with empty days filled in so gaps remain visible. */
export function buildHeatmapDays(year: number, buckets: TimelineBucket[]): HeatmapDay[] {
  if (!Number.isInteger(year) || year < 100 || year > 9998) return [];
  const counts = new Map(buckets.map(bucket => [bucket.date_str, bucket.count]));
  const start = Date.UTC(year, 0, 1);
  const end = Date.UTC(year + 1, 0, 1);
  const offset = (new Date(start).getUTCDay() + 6) % 7;
  const days: HeatmapDay[] = [];
  for (let time = start, index = 0; time < end; time += 86400000, index++) {
    const day = new Date(time);
    const date = day.toISOString().slice(0, 10);
    days.push({ date, count: counts.get(date) ?? 0, week: Math.floor((index + offset) / 7),
      weekday: (index + offset) % 7, month: day.getUTCMonth() + 1, day: day.getUTCDate() });
  }
  return days;
}

/** Four non-zero levels, relative to the busiest day in the displayed year. */
export function heatmapLevel(count: number, max: number): number {
  return count <= 0 || max <= 0 ? 0 : Math.min(4, Math.max(1, Math.ceil(count / max * 4)));
}

/** Intersect chart buckets with an existing selection; changing granularity never expands it. */
export function intersectBucketRange(date: string, unit: Parameters<typeof bucketRange>[1], tzHours: number,
  current?: { start: number; end: number } | null): { start: number; end: number } {
  const range = bucketRange(date, unit, tzHours);
  return current ? { start: Math.max(range.start, current.start), end: Math.min(range.end, current.end) } : range;
}
