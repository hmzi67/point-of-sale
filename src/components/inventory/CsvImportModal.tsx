import { useState } from "react";
import { Download, Upload, X } from "lucide-react";
import { getCsvTemplate, importItemsCsv } from "../../services/inventoryService";
import { downloadTextFile } from "../../utils/csv";
import type { ImportSummary } from "../../types";

interface CsvImportModalProps {
  onClose: () => void;
  /** Called once at least one row imported, so the caller can refresh the list. */
  onImported: () => void;
}

/** Bulk-load a stock list instead of typing every item in by hand — the
 * Phase 14 onboarding aid for a client who already has a spreadsheet.
 * Reads the file entirely client-side (`FileReader`, same base64-avoidance
 * pattern product photos use) and hands the raw text to Rust; nothing about
 * the file ever needs a filesystem-access capability. */
export function CsvImportModal({ onClose, onImported }: CsvImportModalProps) {
  const [fileName, setFileName] = useState<string | null>(null);
  const [isImporting, setIsImporting] = useState(false);
  const [summary, setSummary] = useState<ImportSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  const downloadTemplate = async () => {
    try {
      const template = await getCsvTemplate();
      await downloadTextFile(template, "inventory-import-template.csv");
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const handleFile = async (file: File) => {
    setFileName(file.name);
    setError(null);
    setSummary(null);
    setIsImporting(true);
    try {
      const text = await file.text();
      const result = await importItemsCsv(text);
      setSummary(result);
      if (result.imported > 0) onImported();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsImporting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4">
      <div className="w-full max-w-md rounded-lg bg-surface shadow-xl">
        <div className="flex items-center justify-between border-b border-line px-5 py-4">
          <h3 className="text-sm font-semibold text-fg">Import items from CSV</h3>
          <button type="button" onClick={onClose} className="text-fg-muted hover:text-fg-2">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-4 px-5 py-4">
          <p className="text-sm text-fg-muted">
            Bulk-load an existing stock list. Requires <code className="rounded bg-surface-3 px-1">name</code> and{" "}
            <code className="rounded bg-surface-3 px-1">price</code> columns — everything else (barcode, category,
            cost, stock, low-stock threshold) is optional. Column order and letter case don't matter.
          </p>

          <button
            type="button"
            onClick={() => void downloadTemplate()}
            className="flex items-center gap-1.5 text-xs font-medium text-accent-text hover:text-accent-text"
          >
            <Download className="h-3.5 w-3.5" />
            Download an example CSV
          </button>

          <label className="flex cursor-pointer flex-col items-center gap-2 rounded-md border-2 border-dashed border-line-strong px-4 py-8 text-center hover:border-accent hover:bg-surface-2">
            <Upload className="h-6 w-6 text-fg-muted" />
            <span className="text-sm text-fg-2">
              {fileName ?? "Click to choose a .csv file"}
            </span>
            <input
              type="file"
              accept=".csv,text/csv"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) void handleFile(file);
                e.target.value = "";
              }}
            />
          </label>

          {isImporting && <p className="text-sm text-fg-muted">Importing…</p>}

          {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

          {summary && (
            <div className="space-y-2">
              <p
                className={[
                  "rounded-md px-3 py-2 text-sm",
                  summary.imported > 0 ? "bg-success-soft text-success-text" : "bg-warning-soft text-warning-text",
                ].join(" ")}
              >
                {summary.imported} item{summary.imported === 1 ? "" : "s"} imported.
                {summary.errors.length > 0 && ` ${summary.errors.length} row${summary.errors.length === 1 ? "" : "s"} skipped.`}
              </p>

              {summary.errors.length > 0 && (
                <div className="max-h-40 overflow-y-auto rounded-md border border-line">
                  <table className="min-w-full text-xs">
                    <tbody className="divide-y divide-line">
                      {summary.errors.map((err) => (
                        <tr key={err.row}>
                          <td className="whitespace-nowrap px-2.5 py-1.5 font-medium text-fg-muted">
                            Row {err.row}
                          </td>
                          <td className="px-2.5 py-1.5 text-danger-text">{err.message}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}

          <div className="flex justify-end">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-line-strong px-3 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface-2"
            >
              {summary ? "Done" : "Cancel"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
