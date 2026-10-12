import type { UnlistenFn } from '../../../lib/ipc/events';

/** Retry missing built-in task subscriptions while retaining successful ones. */
export function createBuiltinTaskEvents(subscribe: Array<() => Promise<UnlistenFn>>) {
  const stops = new Map<number, UnlistenFn>();
  let pending: Promise<void> | null = null;
  let disposed = false;
  function ensure(): Promise<void> {
    if (disposed) return Promise.resolve();
    if (pending) return pending;
    const missing = subscribe.flatMap((attach, index) => stops.has(index) ? [] : [{ attach, index }]);
    pending = Promise.allSettled(missing.map(entry => entry.attach())).then(results => {
      results.forEach((result, index) => {
        if (result.status === 'fulfilled') {
          if (disposed) result.value();
          else stops.set(missing[index].index, result.value);
        } else console.warn('订阅内置任务进度失败', result.reason);
      });
      pending = null;
    });
    return pending;
  }
  function dispose() {
    if (disposed) return;
    disposed = true;
    for (const stop of stops.values()) stop();
    stops.clear();
  }
  return { ensure, dispose };
}
