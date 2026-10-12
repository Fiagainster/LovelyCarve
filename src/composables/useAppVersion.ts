import { onMounted, readonly, ref } from 'vue';

// Vite's `define` substitution falls back to literal strings here so
// the file still parses when types haven't propagated, and so tests
// that run outside the Vite pipeline don't blow up on the symbol.
const BUILD_VERSION =
  typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.0.0';
const BUILD_DATE =
  typeof __APP_BUILD_DATE__ !== 'undefined' ? __APP_BUILD_DATE__ : '';

let cachedTauriVersion: string | null = null;

async function loadTauriVersion(): Promise<string | null> {
  if (cachedTauriVersion) return cachedTauriVersion;
  try {
    const mod = await import('@tauri-apps/api/app');
    if (typeof mod.getVersion === 'function') {
      cachedTauriVersion = await mod.getVersion();
      return cachedTauriVersion;
    }
  } catch {
    // Running in a plain browser (Vite preview, vitest, etc.) — no host.
  }
  return null;
}

/**
 * Single-source-of-truth for the version shown in settings.
 *
 * Resolution order:
 *   1. Tauri's `getVersion()` (reads `tauri.conf.json` — the shipped
 *      binary's authoritative version).
 *   2. Compile-time constant from `package.json` (vite `define`).
 *
 * The compile-time value is returned synchronously so the UI never
 * flashes empty; if Tauri is reachable the value is upgraded once.
 */
export function useAppVersion() {
  const version = ref(BUILD_VERSION);
  const buildDate = BUILD_DATE;
  const source = ref<'build' | 'tauri'>('build');

  onMounted(async () => {
    const tauriVersion = await loadTauriVersion();
    if (tauriVersion) {
      version.value = tauriVersion;
      source.value = 'tauri';
    }
  });

  return {
    version: readonly(version),
    buildDate,
    source: readonly(source),
  };
}
