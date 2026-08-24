import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { RootErrorBoundary } from "./components/layout/RootErrorBoundary";
import { SplashGate } from "./components/layout/SplashGate";
import { ping } from "./services/tauriClient";
import { IS_ANDROID } from "./types";
import "./styles.css";

// Fire-and-forget, on every platform, before anything else in this file
// runs — its only job is to land an `app_ping invoked` line in
// `pos-startup.log` so that log is *proof* the main window's JS bundle
// actually started executing, not just that Tauri created the window. See
// `commands::app_ping`'s doc comment for why this specific line matters
// for diagnosing "the app opens and closes with no error" reports.
void ping().catch(() => {
  // Nothing to do here — a failed ping only means the diagnostic marker
  // didn't get logged, never a reason to block the app from rendering.
});

// Desktop's splash is a real native window, already on screen before this
// bundle even starts loading (see tauri.conf.json + lib.rs) — mounting
// SplashGate there too would just be a second, redundant splash. Android has
// no equivalent native window (see tauri.android.conf.json), so SplashGate
// renders the same fade/scale logo in-app instead.
const AppRoot = IS_ANDROID ? (
  <SplashGate>
    <App />
  </SplashGate>
) : (
  <App />
);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RootErrorBoundary>{AppRoot}</RootErrorBoundary>
  </React.StrictMode>,
);
