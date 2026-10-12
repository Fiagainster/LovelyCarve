import { describe, expect, it } from 'vitest';
import {
  compareTaskPriority,
  formatTaskClock,
  formatTaskDateTime,
  formatTaskDuration,
  groupTaskSections,
  parseTaskTime,
  taskProgressPercent,
  taskResultPanel,
} from './taskCenterView';
import type { SchedTaskStatus } from '../../../lib/ipc/tasks';

describe('task center order', () => {
  it('places running work and actionable failures before completed history', () => {
    const statuses: SchedTaskStatus[] = ['success', 'cancel', 'queued', 'created', 'pause', 'offline', 'failed', 'running'];
    expect(statuses.map((status, id) => ({ status, id })).sort(compareTaskPriority).map(task => task.status))
      .toEqual(['running', 'failed', 'offline', 'pause', 'created', 'queued', 'cancel', 'success']);
  });
  it('shows newest first within a priority group without depending on input order', () => {
    const tasks = [2, 5, 1].map(id => ({ id, status: 'running' as const }));
    expect(tasks.sort(compareTaskPriority).map(task => task.id)).toEqual([5, 2, 1]);
    expect(compareTaskPriority(tasks[0], tasks[0])).toBe(0);
  });
  it('orders scheduler UTC times and built-in ISO times on one clock', () => {
    const plugin = { id: 'plugin:1', status: 'success' as const, createAt: '2026-10-02 06:00:00' };
    const builtin = { id: 'search:1', status: 'success' as const, createAt: '2026-10-02T05:30:00.000Z' };
    expect([builtin, plugin].sort(compareTaskPriority).map(task => task.id)).toEqual(['plugin:1', 'search:1']);
  });
});

describe('task sections', () => {
  it('groups running, waiting and paused work apart from failures and history', () => {
    const tasks = (['success', 'failed', 'pause', 'cancel', 'running', 'offline', 'queued'] as const)
      .map((status, id) => ({ id, status }));
    expect(groupTaskSections(tasks).map(section => [section.label, section.tasks.map(task => task.status)])).toEqual([
      ['进行中', ['running', 'pause', 'queued']],
      ['需要处理', ['offline', 'failed']],
      ['已结束', ['cancel', 'success']],
    ]);
    expect(groupTaskSections([{ id: 1, status: 'success' }]).map(section => section.key)).toEqual(['finished']);
  });
});

describe('task result pages', () => {
  const task = (builtin: boolean, analyzer: string, status: SchedTaskStatus, artifactCount = 0) =>
    ({ builtin, status, info: { analyzer }, result: { artifactCount } });
  it('links built-in operations to their page in every state', () => {
    expect(['search', 'timeline', 'carve', 'docker', 'filesystem', 'index'].map(kind => taskResultPanel(task(true, kind, 'failed'))))
      .toEqual(['search-results', 'timeline', 'carve', 'files', 'files', 'files']);
  });
  it('opens analysis results only after a plugin run stored records', () => {
    expect(taskResultPanel(task(false, 'wechat', 'success', 12))).toBe('analysis');
    expect(taskResultPanel(task(false, 'wechat', 'success', 0))).toBeNull();
    expect(taskResultPanel(task(false, 'wechat', 'failed', 12))).toBeNull();
    expect(taskResultPanel({ builtin: false, status: 'success', info: { analyzer: 'wechat' }, result: null })).toBeNull();
  });
});

describe('task progress and duration', () => {
  it('shows completion only for success and preserves interrupted progress', () => {
    expect(taskProgressPercent({ status: 'success', progress: 76 })).toBe(100);
    expect(taskProgressPercent({ status: 'failed', progress: 42 })).toBe(42);
    expect(taskProgressPercent({ status: 'cancel', progress: 0 })).toBe(0);
    expect(taskProgressPercent({ status: 'running', progress: 120 })).toBe(100);
    expect(taskProgressPercent({ status: 'running', progress: NaN })).toBe(0);
  });
  it('formats elapsed time without wrapping long-running jobs at 24 hours', () => {
    expect([null, -1, NaN, 0, 7.9, 3605, 90000].map(formatTaskDuration))
      .toEqual(['—', '—', '—', '00:00:00', '00:00:07', '01:00:05', '25:00:00']);
  });
});

describe('task times', () => {
  it('reads scheduler timestamps as UTC and ISO timestamps as written', () => {
    expect(parseTaskTime('2026-10-02 05:52:07')).toBe(Date.UTC(2026, 9, 2, 5, 52, 7));
    expect(parseTaskTime('2026-10-02T05:52:07.250Z')).toBe(Date.UTC(2026, 9, 2, 5, 52, 7, 250));
    expect(parseTaskTime('')).toBeNaN();
  });
  it('shortens list times by distance from today and keeps unparseable values', () => {
    const now = new Date(2026, 9, 2, 18, 30);
    const at = (...parts: [number, number, number, number, number]) => new Date(...parts).toISOString();
    expect(formatTaskClock(at(2026, 9, 2, 9, 5), now)).toBe('09:05');
    expect(formatTaskClock(at(2026, 8, 28, 23, 40), now)).toBe('09/28 23:40');
    expect(formatTaskClock(at(2025, 11, 31, 8, 0), now)).toBe('2025/12/31 08:00');
    expect(formatTaskClock('不是时间', now)).toBe('不是时间');
    expect(formatTaskClock(null, now)).toBe('—');
    expect(formatTaskDateTime(new Date(2026, 9, 2, 13, 52, 7).toISOString())).toBe('2026/10/02 13:52:07');
  });
});
