// A separate file (not an inline <script>): the app's CSP is
// `script-src 'self'` with no `'unsafe-inline'`.
//
// Wrapped in a function scope on purpose: a top-level `const ipc` would
// collide with the `window.ipc` global wry injects, and WebKit rejects the
// whole script for that (a SyntaxError before a single line runs).
(() => {
  // `__TAURI_INTERNALS__` is always injected; `window.__TAURI__` only exists
  // with `withGlobalTauri`, which is off so the full API isn't a page global.
  const tauri = window.__TAURI_INTERNALS__;
  if (!tauri) return;

  // The installed version, from tauri.conf.json — allowed for this window
  // alone by capabilities/splashscreen.json (`core:app:allow-version`).
  tauri
    .invoke("plugin:app|version")
    .then((version) => {
      document.getElementById("version").textContent = `v${version}`;
    })
    .catch(() => {
      // Cosmetic — the splash is complete without it.
    });

  // Diagnostic marker in pos-startup.log that the splash's assets resolved
  // and it painted (see `commands::app_splashscreen_ready`). Revealing the
  // main window doesn't wait on this — it waits on the app's own first
  // render.
  window.addEventListener("load", () => {
    tauri.invoke("app_splashscreen_ready").catch(() => {});
  });
})();
