import { call } from "./tauriClient";

/**
 * The frontend's two logging paths. There is no console logging in a
 * release build: a `console.*` line there reaches nobody (no attached
 * devtools) and can expose raw error text to anyone who opens them.
 */

/** A crash the user saw — written to Rust's persistent log (the file
 * support asks for), and echoed to the console in dev. Never throws. */
export function reportError(message: string): void {
  if (import.meta.env.DEV) console.error(message);
  call<void>("app_log_frontend_error", { message }).catch(() => undefined);
}

/** A failure the app recovered from (rolled back, retried) — written to the
 * persistent log at WARN, not ERROR. Never throws. */
export function reportWarning(message: string): void {
  if (import.meta.env.DEV) console.warn(message);
  call<void>("app_log_frontend_warning", { message }).catch(() => undefined);
}

/** A recoverable hiccup worth seeing while developing only. */
export function devWarn(message: string, error?: unknown): void {
  if (import.meta.env.DEV) console.warn(message, error);
}
