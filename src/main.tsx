import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { SplashGate } from "./components/layout/SplashGate";
import { IS_ANDROID } from "./types";
import "./styles.css";

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
  <React.StrictMode>{AppRoot}</React.StrictMode>,
);
