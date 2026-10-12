import { readdir, readFile, access } from 'node:fs/promises';
import { join, relative } from 'node:path';
import { auditModule, runtimeCycles } from './lib/frontend-structure.mjs';

const root = process.cwd();
const violations = [];
let files = 0;
let runtimeImports = 0;
let typeImports = 0;
const graph = new Map();
async function* walk(directory) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) yield* walk(path);
    else if (/\.(ts|vue)$/.test(entry.name) && !/\.(test|spec)\./.test(entry.name)) yield path;
  }
}
async function exists(path) {
  try { await access(join(root, path)); return true; } catch { return false; }
}
for await (const path of walk(join(root, 'src'))) {
  const file = relative(root, path).replaceAll('\\', '/');
  const result = auditModule(file, await readFile(path, 'utf8'));
  files++;
  const edges = [];
  for (const violation of result.violations) violations.push(`${file}:${violation.line}: ${violation.message}`);
  for (const entry of result.imports) {
    if (entry.typeOnly) typeImports++; else runtimeImports++;
    if (!entry.target) continue;
    const candidates = [entry.target, `${entry.target}.ts`, `${entry.target}.vue`, `${entry.target}/index.ts`];
    const found = await Promise.all(candidates.map(exists));
    const resolved = candidates[found.findIndex(Boolean)];
    if (!resolved) violations.push(`${file}: unresolved import ${entry.specifier}`);
    else if (!entry.typeOnly && /\.(ts|vue)$/.test(resolved)) edges.push(resolved);
  }
  graph.set(file, edges);
  for (const style of result.styles) if (!style || !await exists(style)) violations.push(`${file}: unresolved scoped style ${style}`);
}
for (const cycle of runtimeCycles(graph)) violations.push(`Runtime import cycle: ${cycle.join(' <-> ')}`);
if (violations.length) {
  console.error(violations.join('\n'));
  process.exitCode = 1;
} else console.log(`Frontend structure OK: ${files} modules; ${runtimeImports} runtime and ${typeImports} type-only imports.`);
