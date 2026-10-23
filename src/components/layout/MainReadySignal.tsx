import { useEffect } from "react";
import { signalMainReady } from "../../services/tauriClient";

/**
 * Desktop: tells Rust the main window has rendered its first screen, so it
 * swaps the splash window for this one (`commands::app_main_ready`). An
 * effect rather than `requestAnimationFrame`: the main window is still
 * hidden at this point, and WebKit pauses animation frames in a hidden
 * window, while effects still run after the commit. Rust's MAX_SPLASH_MS
 * ceiling reveals the window anyway if this never arrives, so a failure here
 * is never fatal. Renders nothing.
 */
export function MainReadySignal() {
  useEffect(() => {
    void signalMainReady().catch(() => {});
  }, []);
  return null;
}
