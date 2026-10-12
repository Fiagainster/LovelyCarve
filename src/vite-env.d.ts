/// <reference types="vite/client" />

declare module "*.vue" {
  import type { DefineComponent } from "vue";
  const component: DefineComponent<{}, {}, any>;
  export default component;
}

// Compile-time constants injected by vite.config.ts. Sourced from
// package.json so the status-bar can show a version without making
// a runtime call when the Tauri host isn't available.
declare const __APP_VERSION__: string;
declare const __APP_BUILD_DATE__: string;
