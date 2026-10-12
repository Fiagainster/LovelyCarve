import { installConsoleCapture } from '../composables/useConsoleCapture';
import { useTheme } from '../composables/useTheme';
import { initUiMotion } from '../composables/useUiMotion';

/** Apply window preferences before either root mounts. */
export function bootstrapWindow() {
  // Apply the persisted (or system) theme before anything renders, so dark and
  // pink users do not see the light defaults fade in on every launch.
  useTheme().initTheme();

  initUiMotion();

  // Capture startup output before mounting; settings can inspect it later.
  installConsoleCapture();

}
