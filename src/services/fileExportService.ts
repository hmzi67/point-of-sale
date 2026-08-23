import { save } from "@tauri-apps/plugin-dialog";
import { writeFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { isTauri } from "./tauriClient";

/**
 * The single boundary for "let the user save an exported file" — mirrors
 * the role `tauriClient.ts` plays for `invoke()`, but for the dialog/fs
 * plugins instead of core commands, so nothing else in the app imports
 * `@tauri-apps/plugin-dialog`/`@tauri-apps/plugin-fs` directly.
 *
 * Every export (CSV report, PDF report) used to go through a `<a
 * download>`-plus-`Blob` trick (`URL.createObjectURL` + a synthetic click).
 * That's a desktop-browser convention Tauri's desktop webviews happen to
 * honor, but Android's WebView does not — tapping "Export" there quietly
 * did nothing. Native `save()` (the OS "Save As" picker — Storage Access
 * Framework on Android, the native panel on desktop) plus `writeFile`
 * writes real bytes to a real, user-chosen location on both platforms, so
 * this is one implementation, not a per-platform branch.
 *
 * Returns `false` (not an error) when the user cancels the save dialog —
 * callers should treat that as "nothing to report", not a failure.
 */
async function pickSavePath(suggestedName: string, extensions: string[]): Promise<string | null> {
  const path = await save({
    defaultPath: suggestedName,
    filters: [{ name: extensions.join("/").toUpperCase(), extensions }],
  });
  return path ?? null;
}

export async function saveTextFile(contents: string, suggestedName: string, extension: string): Promise<boolean> {
  // `npm run dev` in a plain browser tab has no Tauri plugins to call —
  // fall back to the old blob-link trick there so local dev still works.
  if (!isTauri()) {
    downloadViaBlobLink(contents, suggestedName);
    return true;
  }
  const path = await pickSavePath(suggestedName, [extension]);
  if (!path) return false;
  await writeTextFile(path, contents);
  return true;
}

export async function saveBinaryFile(
  bytes: Uint8Array,
  suggestedName: string,
  extension: string,
): Promise<boolean> {
  if (!isTauri()) {
    downloadViaBlobLink(bytes, suggestedName);
    return true;
  }
  const path = await pickSavePath(suggestedName, [extension]);
  if (!path) return false;
  await writeFile(path, bytes);
  return true;
}

/** Browser-only fallback (`npm run dev`, no Tauri runtime) — never used
 * inside the actual desktop/Android app, where `saveTextFile`/
 * `saveBinaryFile` above always take the native path instead. */
function downloadViaBlobLink(content: string | Uint8Array, filename: string): void {
  const blob = new Blob([content]);
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}
