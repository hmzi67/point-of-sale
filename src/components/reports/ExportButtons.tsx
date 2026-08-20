import { useState } from "react";
import { Download, FileText, Printer } from "lucide-react";
import {
  getCategorySales,
  getTableSalesSummary,
  printCategorySalesThermal,
  printTableSalesThermal,
} from "../../services/reportsService";
import { downloadReportCsv, downloadTableSalesCsv } from "../../utils/reportCsv";
import type { AppConfig, DailySales, SalesSummary, TopItem } from "../../types";

interface ExportButtonsProps {
  summary: SalesSummary | null;
  topItems: TopItem[];
  series: DailySales[];
  config: AppConfig;
  startDate: string;
  endDate: string;
  /** Hides the Table Wise Sales export buttons entirely when the `tables`
   * module isn't enabled — same "not just empty data, not offered at all"
   * treatment the report view itself gets. */
  tablesEnabled: boolean;
}

export function ExportButtons({ summary, topItems, series, config, startDate, endDate, tablesEnabled }: ExportButtonsProps) {
  const [isPreparingPdf, setIsPreparingPdf] = useState(false);
  const [isPreparingCategoryPdf, setIsPreparingCategoryPdf] = useState(false);
  const [isPrintingCategory, setIsPrintingCategory] = useState(false);
  const [isPreparingTablePdf, setIsPreparingTablePdf] = useState(false);
  const [isPreparingTableCsv, setIsPreparingTableCsv] = useState(false);
  const [isPrintingTable, setIsPrintingTable] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const disabled = !summary;

  const exportPdf = async () => {
    if (!summary) return;
    setIsPreparingPdf(true);
    setError(null);
    try {
      // jsPDF + autoTable is a large chunk with no reason to load until a
      // report is actually exported — deferred the same way the receipt is.
      const { downloadReportPdf } = await import("../../utils/reportPdf");
      downloadReportPdf({ summary, topItems, series, config });
    } catch (e) {
      // Previously uncaught — a build failure was a silent unhandled
      // rejection with the button just going back to normal and no
      // indication anything went wrong (Phase 13 error-handling review).
      setError(`Could not prepare the PDF: ${(e as Error).message}`);
    } finally {
      setIsPreparingPdf(false);
    }
  };

  const exportCsv = () => {
    if (!summary) return;
    setError(null);
    try {
      downloadReportCsv({ summary, topItems, series, config });
    } catch (e) {
      setError(`Could not prepare the CSV: ${(e as Error).message}`);
    }
  };

  const exportCategoryPdf = async () => {
    setIsPreparingCategoryPdf(true);
    setError(null);
    try {
      const report = await getCategorySales(startDate, endDate);
      // jsPDF + autoTable is a large chunk with no reason to load until a
      // report is actually exported — same deferral as the summary PDF.
      const { downloadCategorySalesPdf } = await import("../../utils/categorySalesPdf");
      downloadCategorySalesPdf(report, config);
    } catch (e) {
      setError(`Could not prepare the Category Wise Sale PDF: ${(e as Error).message}`);
    } finally {
      setIsPreparingCategoryPdf(false);
    }
  };

  const printCategoryThermal = async () => {
    setIsPrintingCategory(true);
    setError(null);
    try {
      await printCategorySalesThermal(startDate, endDate);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsPrintingCategory(false);
    }
  };

  const exportTableSalesPdf = async () => {
    setIsPreparingTablePdf(true);
    setError(null);
    try {
      const report = await getTableSalesSummary(startDate, endDate);
      const { downloadTableSalesPdf } = await import("../../utils/tableSalesPdf");
      downloadTableSalesPdf(report, config);
    } catch (e) {
      setError(`Could not prepare the Table Wise Sales PDF: ${(e as Error).message}`);
    } finally {
      setIsPreparingTablePdf(false);
    }
  };

  const exportTableSalesCsv = async () => {
    setIsPreparingTableCsv(true);
    setError(null);
    try {
      const report = await getTableSalesSummary(startDate, endDate);
      downloadTableSalesCsv(report);
    } catch (e) {
      setError(`Could not prepare the Table Wise Sales CSV: ${(e as Error).message}`);
    } finally {
      setIsPreparingTableCsv(false);
    }
  };

  const printTableSalesThermalHandler = async () => {
    setIsPrintingTable(true);
    setError(null);
    try {
      await printTableSalesThermal(startDate, endDate);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsPrintingTable(false);
    }
  };

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex flex-wrap justify-end gap-2">
        <button
          type="button"
          onClick={() => void exportPdf()}
          disabled={disabled || isPreparingPdf}
          className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <FileText className="h-4 w-4" />
          {isPreparingPdf ? "Preparing…" : "Export PDF"}
        </button>
        <button
          type="button"
          onClick={exportCsv}
          disabled={disabled}
          className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Download className="h-4 w-4" />
          Export CSV
        </button>
        <button
          type="button"
          onClick={() => void exportCategoryPdf()}
          disabled={disabled || isPreparingCategoryPdf}
          className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <FileText className="h-4 w-4" />
          {isPreparingCategoryPdf ? "Preparing…" : "Category Wise Sale (PDF)"}
        </button>
        <button
          type="button"
          onClick={() => void printCategoryThermal()}
          disabled={disabled || isPrintingCategory}
          className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <Printer className="h-4 w-4" />
          {isPrintingCategory ? "Printing…" : "Print Category Wise Sale"}
        </button>
        {tablesEnabled && (
          <>
            <button
              type="button"
              onClick={() => void exportTableSalesPdf()}
              disabled={disabled || isPreparingTablePdf}
              className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <FileText className="h-4 w-4" />
              {isPreparingTablePdf ? "Preparing…" : "Table Wise Sales (PDF)"}
            </button>
            <button
              type="button"
              onClick={() => void exportTableSalesCsv()}
              disabled={disabled || isPreparingTableCsv}
              className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Download className="h-4 w-4" />
              {isPreparingTableCsv ? "Preparing…" : "Table Wise Sales (CSV)"}
            </button>
            <button
              type="button"
              onClick={() => void printTableSalesThermalHandler()}
              disabled={disabled || isPrintingTable}
              className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Printer className="h-4 w-4" />
              {isPrintingTable ? "Printing…" : "Print Table Wise Sales"}
            </button>
          </>
        )}
      </div>
      {error && <p className="text-xs text-red-600">{error}</p>}
    </div>
  );
}
