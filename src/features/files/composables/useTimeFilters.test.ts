import { describe, expect, it } from 'vitest';
import type { DiskFsNode } from '../model/model';
import { useTimeFilters } from './useTimeFilters';

function node(timestamps: Partial<DiskFsNode['timestamps']>): DiskFsNode {
  return {
    id: 'n',
    name: 'n',
    parent_id: null,
    kind: 'file',
    size: 0,
    deleted: false,
    inode: null,
    timestamps: {
      modified: null,
      accessed: null,
      created: null,
      mft_changed: null,
      birth: null,
      ...timestamps,
    },
  };
}

function filters() {
  return useTimeFilters({ filterKey: () => 'modified', getPopoverElement: () => null });
}

describe('useTimeFilters', () => {
  it('expands date-only bounds to whole days and matches inclusively', () => {
    const time = filters();
    Object.assign(time.timeFilters.modified, {
      fromEnabled: true, fromValue: '2024-03-01', toEnabled: true, toValue: '2024-03-02',
    });
    time.applyTimeFilterForKey('modified');

    expect(time.timeFilters.modified.appliedFrom).toBe('2024-03-01 00:00:00');
    expect(time.timeFilters.modified.appliedTo).toBe('2024-03-02 23:59:59');
    expect(time.activeTimeFilterKeys.value).toEqual(['modified']);
    expect(time.matchesTimeFilter(node({ modified: '2024-03-02T23:59:59Z' }), 'modified')).toBe(true);
    expect(time.matchesTimeFilter(node({ modified: '2024-03-03T00:00:00Z' }), 'modified')).toBe(false);
    // Rows without a timestamp never satisfy an active bound.
    expect(time.matchesTimeFilter(node({}), 'modified')).toBe(false);
  });

  it('rejects reversed bounds without changing the applied filter', () => {
    const time = filters();
    Object.assign(time.timeFilters.modified, {
      fromEnabled: true, fromValue: '2024-05-01 12:00', toEnabled: true, toValue: '2024-04-01',
    });
    expect(time.applyTimeFilterForKey('modified')).toBe(false);

    expect([time.timeFilters.modified.appliedFrom, time.timeFilters.modified.appliedTo])
      .toEqual([null, null]);
    expect(time.timeFilterErrors.value.from).toContain('不能晚于');
  });

  it.each(['2024-02-30', '2024-13-01', '2024-01-01 24:00', '2024-01-01 12:60', '2024-01-01 12:00:60'])('rejects invalid calendar values: %s', value => {
    const time = filters();
    Object.assign(time.timeFilters.modified, { fromEnabled: true, fromValue: '2024-02-29' });
    expect(time.applyTimeFilterForKey('modified')).toBe(true);
    time.timeFilters.modified.fromValue = value;
    expect(time.applyTimeFilterForKey('modified')).toBe(false);
    expect(time.timeFilterErrors.value.from).toContain('有效日期');
    expect(time.timeFilters.modified.appliedFrom).toBe('2024-02-29 00:00:00');
  });

  it('falls back to the birth time for the created column and ignores malformed bounds', () => {
    const time = filters();
    Object.assign(time.timeFilters.created, { fromEnabled: true, fromValue: '2023-01-01' });
    time.applyTimeFilterForKey('created');
    expect(time.matchesTimeFilter(node({ birth: '2023-06-01T00:00:00Z' }), 'created')).toBe(true);

    Object.assign(time.timeFilters.accessed, { fromEnabled: true, fromValue: 'yesterday' });
    time.applyTimeFilterForKey('accessed');
    expect(time.timeFilters.accessed.active).toBe(false);
    expect(time.matchesTimeFilter(node({}), 'accessed')).toBe(true);
  });
});
