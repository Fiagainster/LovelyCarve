import { copyFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const projectRoot = fileURLToPath(new URL('..', import.meta.url));
const iconsDirectory = join(projectRoot, 'src-tauri', 'icons');
const assetsDirectory = join(projectRoot, 'src', 'assets');

function generateIcons(source, args = []) {
  const result = spawnSync(process.execPath, ['run', 'tauri', 'icon', source, ...args], {
    cwd: projectRoot,
    stdio: 'inherit',
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`图标生成失败：${source}（${result.signal ?? result.status}）`);
}

generateIcons(join(iconsDirectory, 'source.png'));
copyFileSync(join(iconsDirectory, 'icon.png'), join(assetsDirectory, 'app-icon.png'));

const temporaryDirectory = mkdtempSync(join(tmpdir(), 'lovelycarve-icons-'));
try {
  generateIcons(join(iconsDirectory, 'source-background.png'), ['--output', temporaryDirectory, '--png', '512']);
  copyFileSync(join(temporaryDirectory, '512x512.png'), join(assetsDirectory, 'app-icon-background.png'));
} finally {
  rmSync(temporaryDirectory, { recursive: true, force: true });
}
