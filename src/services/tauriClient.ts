import { invoke } from "@tauri-apps/api/core";

/**
 * The single boundary between React and Rust.
 *
 * Components never import `invoke` directly — they call the typed helpers
 * exported from `src/services/*`, which all funnel through `call()` below.
 * That keeps command names, argument casing (camelCase here, snake_case in
 * Rust) and error handling in one place.
 *
 * Command naming convention: `<module>_<action>` — e.g. `inventory_add_item`,
 * `billing_create_sale`, `config_get_enabled_modules`.
 */

export class TauriCommandError extends Error {
  constructor(
    public readonly command: string,
    message: string,
    public readonly cause?: unknown,
  ) {
    super(message);
    this.name = "TauriCommandError";
  }
}

/** True when running inside the Tauri webview rather than a plain browser (`npm run dev`). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function call<TResult>(
  command: string,
  args?: Record<string, unknown>,
): Promise<TResult> {
  try {
    return await invoke<TResult>(command, args);
  } catch (error) {
    // Rust command errors arrive as plain strings; anything else is a transport failure.
    const message =
      typeof error === "string" ? error : (error as Error)?.message ?? String(error);
    throw new TauriCommandError(command, message, error);
  }
}

/**
 * Same as `call`, but returns `fallback` instead of throwing. Use for
 * non-critical reads (e.g. a dashboard widget) — never for a sale write, where
 * the cashier must see the failure.
 */
export async function callSafe<TResult>(
  command: string,
  fallback: TResult,
  args?: Record<string, unknown>,
): Promise<TResult> {
  try {
    return await call<TResult>(command, args);
  } catch (error) {
    console.error("[tauriClient] command failed:", error);
    return fallback;
  }
}

/** Health check wired to the Rust `app_ping` command — proves the IPC bridge works. */
export function ping(): Promise<string> {
  return call<string>("app_ping");
}
