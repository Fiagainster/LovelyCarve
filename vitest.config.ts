import { defineConfig, mergeConfig } from 'vitest/config';
import viteConfig from './vite.config.ts';

export default mergeConfig(viteConfig, defineConfig({
  test: {
    setupFiles: ['scripts/test-setup.mjs'],
    // Backend build/probe output may contain unpacked third-party test suites.
    include: ['src/**/*.{test,spec}.{ts,tsx,js,jsx}'],
  },
}));
