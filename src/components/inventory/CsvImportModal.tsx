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
      downloadTextFile(template, "inventory-import-template.csv", "text/csv");
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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-md rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
          <h3 className="text-sm font-semibold text-slate-900">Import items from CSV</h3>
          <button type="button" onClick={onClose} className="text-slate-400 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-4 px-5 py-4">
          <p className="text-sm text-slate-500">
            Bulk-load an existing stock list. Requires <code className="rounded bg-slate-100 px-1">name</code> and{" "}
            <code className="rounded bg-slate-100 px-1">price</code> columns — everything else (barcode, category,
            cost, stock, low-stock threshold) is optional. Column order and letter case don't matter.
          </p>

          <button
            type="button"
            onClick={() => void downloadTemplate()}
            className="flex items-center gap-1.5 text-xs font-medium text-brand-600 hover:text-brand-700"
          >
            <Download className="h-3.5 w-3.5" />
            Download an example CSV
          </button>

          <label className="flex cursor-pointer flex-col items-center gap-2 rounded-md border-2 border-dashed border-slate-300 px-4 py-8 text-center hover:border-brand-400 hover:bg-slate-50">
            <Upload className="h-6 w-6 text-slate-400" />
            <span className="text-sm text-slate-600">
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

          {isImporting && <p className="text-sm text-slate-500">Importing…</p>}

          {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

          {summary && (
            <div className="space-y-2">
              <p
                className={[
                  "rounded-md px-3 py-2 text-sm",
                  summary.imported > 0 ? "bg-emerald-50 text-emerald-700" : "bg-amber-50 text-amber-700",
                ].join(" ")}
              >
                {summary.imported} item{summary.imported === 1 ? "" : "s"} imported.
                {summary.errors.length > 0 && ` ${summary.errors.length} row${summary.errors.length === 1 ? "" : "s"} skipped.`}
              </p>

              {summary.errors.length > 0 && (
                <div className="max-h-40 overflow-y-auto rounded-md border border-slate-200">
                  <table className="min-w-full text-xs">
                    <tbody className="divide-y divide-slate-100">
                      {summary.errors.map((err) => (
                        <tr key={err.row}>
                          <td className="whitespace-nowrap px-2.5 py-1.5 font-medium text-slate-500">
                            Row {err.row}
                          </td>
                          <td className="px-2.5 py-1.5 text-red-600">{err.message}</td>
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
              className="rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-600 hover:bg-slate-50"
            >
              {summary ? "Done" : "Cancel"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
