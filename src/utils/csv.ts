import { saveTextFile } from "../services/fileExportService";

/** Minimal CSV builder — quotes any field containing a comma, quote or
 * newline, doubling embedded quotes per RFC 4180. Dependency-free since this
 * is the only place in the app that needs it. */
export function toCsvRow(fields: Array<string | number>): string {
  return fields
    .map((field) => {
      const str = String(field);
      return /[",\n]/.test(str) ? `"${str.replace(/"/g, '""')}"` : str;
    })
    .join(",");
}

export function buildCsv(rows: Array<Array<string | number>>): string {
  return rows.map(toCsvRow).join("\r\n");
}

/** Opens the native "Save As" dialog and writes `content` to wherever the
 * user picks — see `fileExportService.ts` for why this isn't the old
 * browser `<a download>` trick any more (it never worked on Android).
 * Resolves `true` if a file was actually written, `false` if the user
 * cancelled the save dialog. */
export function downloadTextFile(content: string, filename: string): Promise<boolean> {
  const extension = filename.split(".").pop() ?? "csv";
  return saveTextFile(content, filename, extension);
}
