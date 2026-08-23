import type { jsPDF } from "jspdf";
import { saveBinaryFile } from "../services/fileExportService";

/** Native "Save As" for a built jsPDF document — replaces jsPDF's own
 * `.save()`, which uses the same browser-only `<a download>`/blob trick as
 * the old CSV export (see `fileExportService.ts`) and silently does
 * nothing on Android. Resolves `true` if a file was actually written,
 * `false` if the user cancelled the save dialog. */
export function downloadPdf(doc: jsPDF, filename: string): Promise<boolean> {
  const bytes = new Uint8Array(doc.output("arraybuffer"));
  return saveBinaryFile(bytes, filename, "pdf");
}
