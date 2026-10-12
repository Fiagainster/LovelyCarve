// 一次性填充 public/file-viewer（Flyfish File Viewer 运行时资产）。
//
// 背景:file-viewer vite-plugin 的 copyAssets 默认每次 dev 启动都在
// configureServer 里全量重写 ~2881 个文件(~160MB),对 watcher 产生数千条
// 事件,实测把 vite 启动从 ~5s 拖到 ~170s。因此 vite.config.ts 把拷贝改为
// `mode: 'build'`(生产构建照常拷贝到 dist),dev 不再拷贝;本脚本在 `just dev`
// / `just dev-fe` 前执行,按 file-viewer-copy-assets 包版本判断:
//   - public/file-viewer 不存在或版本戳不同 → 全量重拷并写版本戳
//   - 版本一致 → 立即退出(毫秒级)
// 升级 @file-viewer/* 依赖后版本戳自动失配,下次 dev 会重新同步。
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const projectRoot = fileURLToPath(new URL('..', import.meta.url));
const publicDir = join(projectRoot, 'public');
const targetRoot = join(publicDir, 'file-viewer');
const stampPath = join(targetRoot, '.lc-assets-version');

const pkgJsonPath = require.resolve('file-viewer-copy-assets/package.json');
const sourceRoot = join(dirname(pkgJsonPath), 'viewer');
const version = JSON.parse(readFileSync(pkgJsonPath, 'utf8')).version;

if (!existsSync(join(sourceRoot, 'flyfish-viewer-manifest.json'))) {
  console.error(`[file-viewer-assets] 未找到资产源 ${sourceRoot},请先 bun install`);
  process.exit(1);
}

if (existsSync(stampPath)) {
  try {
    if (readFileSync(stampPath, 'utf8').trim() === version) {
      process.exit(0); // 版本未变,无需拷贝
    }
  } catch {
    // 戳损坏,重拷
  }
}

const t0 = Date.now();
rmSync(targetRoot, { recursive: true, force: true });
mkdirSync(targetRoot, { recursive: true });
cpSync(sourceRoot, targetRoot, { recursive: true });
writeFileSync(stampPath, version);
console.log(`[file-viewer-assets] 已同步 file-viewer-copy-assets@${version} → public/file-viewer (${Date.now() - t0}ms)`);
