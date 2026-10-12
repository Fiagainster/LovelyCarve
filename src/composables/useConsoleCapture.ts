import { ref } from 'vue';

export type ConsoleLevel = 'log' | 'info' | 'warn' | 'error' | 'debug';

export interface ConsoleEntry {
  id: number;
  timestamp: number;
  level: ConsoleLevel;
  /** Pre-serialized argument list, one string per console arg. */
  parts: string[];
  /** Joined, cached for cheap display & search. */
  text: string;
}

const MAX_ENTRIES = 5000;
const LEVELS: ConsoleLevel[] = ['log', 'info', 'warn', 'error', 'debug'];

const entries = ref<ConsoleEntry[]>([]);

let nextId = 1;
let installed = false;

function serializeArg(arg: unknown): string {
  if (arg === undefined) return 'undefined';
  if (arg === null) return 'null';
  if (typeof arg === 'string') return arg;
  if (typeof arg === 'number' || typeof arg === 'boolean' || typeof arg === 'bigint') {
    return String(arg);
  }
  if (arg instanceof Error) {
    return arg.stack || `${arg.name}: ${arg.message}`;
  }
  try {
    const seen = new WeakSet<object>();
    return JSON.stringify(arg, (_k, v) => {
      if (typeof v === 'object' && v !== null) {
        if (seen.has(v)) return '[Circular]';
        seen.add(v);
      }
      if (typeof v === 'bigint') return v.toString() + 'n';
      if (typeof v === 'function') return `[Function${v.name ? ` ${v.name}` : ''}]`;
      return v;
    }, 2) ?? String(arg);
  } catch {
    try {
      return String(arg);
    } catch {
      return '[Unserializable]';
    }
  }
}

function pushEntry(level: ConsoleLevel, parts: string[], timestamp = Date.now()) {
  const entry: ConsoleEntry = {
    id: nextId++,
    timestamp,
    level,
    parts,
    text: parts.join(' '),
  };
  const list = entries.value;
  list.push(entry);
  if (list.length > MAX_ENTRIES) {
    list.splice(0, list.length - MAX_ENTRIES);
  }
}

function pushFromConsole(level: ConsoleLevel, args: unknown[]) {
  pushEntry(level, args.map(serializeArg));
}

export function installConsoleCapture() {
  if (installed) return;
  installed = true;

  /* eslint-disable no-console -- wrapping console.* is the whole point of this module */
  for (const level of LEVELS) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      try {
        pushFromConsole(level, args);
      } catch {
        // Never let capture break the original call.
      }
      original(...args);
    };
  }
  /* eslint-enable no-console */

  window.addEventListener('error', (ev) => {
    const detail = ev.error instanceof Error ? ev.error : ev.message;
    pushFromConsole('error', ['[window.error]', detail, ev.filename ? `@ ${ev.filename}:${ev.lineno}:${ev.colno}` : '']);
  });

  window.addEventListener('unhandledrejection', (ev) => {
    pushFromConsole('error', ['[unhandledrejection]', ev.reason]);
  });
}

export function useConsoleCapture() {
  function clear() {
    entries.value = [];
  }
  async function copyAll(): Promise<void> {
    const text = entries.value
      .map((e) => `[${new Date(e.timestamp).toISOString()}] [${e.level.toUpperCase()}] ${e.text}`)
      .join('\n');
    await navigator.clipboard.writeText(text);
  }

  return {
    entries,
    clear,
    copyAll,
  };
}
