import { createApp, type Component } from "vue";
import "./styles/global.css";

import { bootstrapWindow } from "./app/bootstrap";

bootstrapWindow();

// Multi-window dispatch. Tauri loads the same index.html in every window;
// the detached file preview passes `?view=file-preview` so the main app's
// chrome (sidebar, status bar, tab manager) is never constructed there. Both
// roots are dynamic imports so neither window pulls in the other's bundle.
const view = new URLSearchParams(window.location.search).get("view");

async function mount() {
  const root: Component = view === "file-preview"
    ? (await import("./views/FilePreviewWindow.vue")).default
    : (await import("./App.vue")).default;
  createApp(root).mount("#app");
}

void mount();
