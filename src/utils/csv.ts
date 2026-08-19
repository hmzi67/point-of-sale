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

/** Triggers a browser download of `content` as a file named `filename`. */
export function downloadTextFile(content: string, filename: string, mimeType: string): void {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}
