// @vitest-environment happy-dom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { notifySuccess, pauseToast, resumeToast, toastDuration, useToast } from './useToast';

const { toasts, removeToast } = useToast();
const isShown = (id: number) => toasts.value.some(toast => toast.id === id);

describe('toastDuration', () => {
  it('keeps short messages at the base duration', () => {
    expect(toastDuration(3200, '已复制', '文件路径')).toBe(3200);
  });

  it('gives long messages time to be read, up to ten seconds', () => {
    expect(toastDuration(4200, '打开失败', 'x'.repeat(60))).toBe(1800 + 64 * 90);
    expect(toastDuration(3200, '标题', 'x'.repeat(500))).toBe(10_000);
  });
});

describe('timed toasts', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    for (const toast of [...toasts.value]) removeToast(toast.id);
    vi.useRealTimers();
  });

  it('dismiss themselves after their duration', () => {
    const id = notifySuccess('已复制', '文件路径');
    vi.advanceTimersByTime(3199);
    expect(isShown(id)).toBe(true);
    vi.advanceTimersByTime(1);
    expect(isShown(id)).toBe(false);
  });

  it('hold while paused and finish the remaining time after resuming', () => {
    const id = notifySuccess('已复制', '文件路径');
    vi.advanceTimersByTime(1000);
    pauseToast(id);
    vi.advanceTimersByTime(60_000);
    expect(isShown(id)).toBe(true);
    resumeToast(id);
    vi.advanceTimersByTime(2199);
    expect(isShown(id)).toBe(true);
    vi.advanceTimersByTime(1);
    expect(isShown(id)).toBe(false);
  });

  it('leave a grace period when resumed just before the end', () => {
    const id = notifySuccess('已复制', '文件路径');
    vi.advanceTimersByTime(3100);
    pauseToast(id);
    resumeToast(id);
    vi.advanceTimersByTime(1499);
    expect(isShown(id)).toBe(true);
    vi.advanceTimersByTime(1);
    expect(isShown(id)).toBe(false);
  });
});
