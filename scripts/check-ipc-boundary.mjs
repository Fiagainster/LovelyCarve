#!/usr/bin/env bun
import { readdir, readFile } from 'node:fs/promises';
import { join, relative } from 'node:path';

const root = process.cwd();
const sourceRoot = join(root, 'src');
const forbidden = [
  '@tauri-apps/api/core',
  '@tauri-apps/api/event',
];

const whitelist = new Set([
  'src/lib/ipc/client.ts',
  'src/lib/ipc/events.ts',
]);

async function* walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if (entry.name === 'node_modules' || entry.name === 'dist') continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      yield* walk(full);
      continue;
    }
    if (/\.(ts|vue)$/.test(entry.name)) yield full;
  }
}

const violations = [];
for await (const file of walk(sourceRoot)) {
  const rel = relative(root, file).replaceAll('\\', '/');
  if (rel.startsWith('src/lib/ipc/')) continue;
  if (whitelist.has(rel)) continue;
  const text = await readFile(file, 'utf8');
  const lines = text.split(/\r?\n/);
  lines.forEach((line, index) => {
    if (forbidden.some(pattern => line.includes(pattern))) {
      violations.push(`${rel}:${index + 1}: ${line.trim()}`);
    }
  });
}

if (violations.length > 0) {
  console.error('Direct Tauri core/event imports must go through src/lib/ipc/*.');
  console.error(violations.join('\n'));
  process.exit(1);
}

console.log('IPC boundary OK: no direct core/event imports outside src/lib/ipc.');
