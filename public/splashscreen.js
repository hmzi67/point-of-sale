// A separate file (not an inline <script>) because the app's CSP is
// `script-src 'self'` with no `'unsafe-inline'` — same-origin external
// scripts are allowed, inline ones would just be silently blocked.
//
// Tells the Rust side this window has actually painted (see
// `commands::splashscreen_ready`) — the real "ready" signal the splash
// timing waits on, rather than guessing how long a cold webview process
// takes to render its first frame. Fires on `load` (after the logo image
// itself has decoded and is visibly on screen), not any earlier point.
window.addEventListener("load", () => {
  window.__TAURI__?.core?.invoke("splashscreen_ready")?.catch(() => {
    // Nothing to do — the MAX_SPLASH_MS ceiling in lib.rs::run still
    // reveals the main window regardless, so a failed/unreachable IPC
    // bridge here can't strand anyone on the splash.
  });
});
