import { useState } from "react";
import { Download, FileText, Layers, Printer } from "lucide-react";
import {
  getCategorySales,
  getFullReport,
  getProductSalesSummary,
  getRefundsSummary,
  getTableSalesSummary,
  printCategorySalesThermal,
  printFullReportThermal,
  printRefundsSummaryThermal,
  printTableSalesThermal,
} from "../../services/reportsService";
import { downloadReportCsv } from "../../utils/reportCsv";
import type { AppConfig, DailySales, SalesSummary, TopItem, TopItemSort } from "../../types";

type View = "overview" | "products" | "tables" | "refunds";

interface ExportButtonsProps {
  view: View;
  summary: SalesSummary | null;
  topItems: TopItem[];
  series: DailySales[];
  config: AppConfig;
  startDate: string;
  endDate: string;
  /** Hides the Table Wise Sales action group entirely when the `tables`
   * module isn't enabled — same "not just empty data, not offered at all"
   * treatment the report view itself gets. */
  tablesEnabled: boolean;
  /** The Product Wise Sales view's current category filter/sort — its
   * export matches whatever the on-screen table is currently showing. */
  productCategoryId: number | null;
  productSort: TopItemSort;
}

/** One "Download" + (optionally) "Print" pair, grouped and labeled under a
 * single card — the redesign's answer to what used to be five flat,
 * same-looking buttons of mixed purpose. `accent` marks the standalone
 * "Generate Full Report" card so it reads as the distinct, comprehensive
 * option rather than one more per-report action. */
function ActionCard({
  title,
  icon: Icon,
  onDownload,
  isDownloading,
  onPrint,
  isPrinting,
  accent,
}: {
  title: string;
  icon: typeof FileText;
  onDownload: () => void;
  isDownloading: boolean;
  onPrint?: () => void;
  isPrinting?: boolean;
  accent?: boolean;
}) {
  return (
    <div
      className={[
        "flex items-center gap-3 rounded-2xl border p-2.5 shadow-soft",
        accent ? "border-brand-200 bg-brand-50" : "border-slate-200 bg-white",
      ].join(" ")}
    >
      <div
        className={[
          "flex h-8 w-8 shrink-0 items-center justify-center rounded-full",
          accent ? "bg-brand-600 text-white" : "bg-slate-100 text-slate-500",
        ].join(" ")}
      >
        <Icon className="h-4 w-4" />
      </div>
      <div className="min-w-0">
        <p className={["text-xs font-semibold uppercase tracking-wide", accent ? "text-brand-700" : "text-slate-500"].join(" ")}>
          {title}
        </p>
        <div className="mt-1 flex items-center gap-1.5">
          <button
            type="button"
            onClick={onDownload}
            disabled={isDownloading}
            className={[
              "flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50",
              accent ? "bg-brand-600 text-white hover:bg-brand-700" : "text-slate-600 hover:bg-slate-100",
            ].join(" ")}
          >
            <Download className="h-3.5 w-3.5" />
            {isDownloading ? "Preparing…" : "Download"}
          </button>
          {onPrint && (
            <button
              type="button"
              onClick={onPrint}
              disabled={isPrinting}
              className={[
                "flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50",
                accent ? "text-brand-700 hover:bg-brand-100" : "text-slate-600 hover:bg-slate-100",
              ].join(" ")}
            >
              <Printer className="h-3.5 w-3.5" />
              {isPrinting ? "Printing…" : "Print"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

export function ExportButtons({
  view,
  summary,
  topItems,
  series,
  config,
  startDate,
  endDate,
  tablesEnabled,
  productCategoryId,
  productSort,
}: ExportButtonsProps) {
  const [isDownloadingCurrent, setIsDownloadingCurrent] = useState(false);
  const [isDownloadingCategory, setIsDownloadingCategory] = useState(false);
  const [isPrintingCategory, setIsPrintingCategory] = useState(false);
  const [isDownloadingFull, setIsDownloadingFull] = useState(false);
  const [isPrintingFull, setIsPrintingFull] = useState(false);
  // Shared by every tab-aware "current view" print button (Table Wise
  // Sales, Refunds) — only one of them is ever visible/clickable at a time
  // since only one tab is active.
  const [isPrintingCurrentView, setIsPrintingCurrentView] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const disabled = !summary;

  // --- The active tab's own Download (+ Print, where one exists) --------
  const downloadCurrentView = async () => {
    setIsDownloadingCurrent(true);
    setError(null);
    try {
      if (view === "overview") {
        if (!summary) return;
        await downloadReportCsv({ summary, topItems, series, config });
      } else if (view === "products") {
        const report = await getProductSalesSummary(startDate, endDate, productCategoryId, productSort);
        const { downloadProductSalesPdf } = await import("../../utils/productSalesPdf");
        await downloadProductSalesPdf(report, config);
      } else if (view === "tables") {
        const report = await getTableSalesSummary(startDate, endDate);
        const { downloadTableSalesPdf } = await import("../../utils/tableSalesPdf");
        await downloadTableSalesPdf(report, config);
      } else {
        const report = await getRefundsSummary(startDate, endDate);
        const { downloadRefundsSummaryPdf } = await import("../../utils/refundsSummaryPdf");
        await downloadRefundsSummaryPdf(report, config);
      }
    } catch (e) {
      setError(`Could not prepare the download: ${(e as Error).message}`);
    } finally {
      setIsDownloadingCurrent(false);
    }
  };

  const printCurrentView = async () => {
    setIsPrintingCurrentView(true);
    setError(null);
    try {
      if (view === "refunds") {
        await printRefundsSummaryThermal(startDate, endDate);
      } else {
        await printTableSalesThermal(startDate, endDate);
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsPrintingCurrentView(false);
    }
  };

  // --- Category Wise Sale — no dedicated on-screen tab, so it's always
  // offered as its own card rather than folded into the tab-aware one. ---
  const downloadCategorySales = async () => {
    setIsDownloadingCategory(true);
    setError(null);
    try {
      const report = await getCategorySales(startDate, endDate);
      const { downloadCategorySalesPdf } = await import("../../utils/categorySalesPdf");
      await downloadCategorySalesPdf(report, config);
    } catch (e) {
      setError(`Could not prepare the Category Wise Sale download: ${(e as Error).message}`);
    } finally {
      setIsDownloadingCategory(false);
    }
  };

  const printCategorySales = async () => {
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

  // --- Generate Full Report — one consolidated document, distinct from
  // every per-report action above. ---------------------------------------
  const downloadFullReport = async () => {
    setIsDownloadingFull(true);
    setError(null);
    try {
      const report = await getFullReport(startDate, endDate);
      const { downloadFullReportPdf } = await import("../../utils/fullReportPdf");
      await downloadFullReportPdf(report, config);
    } catch (e) {
      setError(`Could not prepare the Full Report: ${(e as Error).message}`);
    } finally {
      setIsDownloadingFull(false);
    }
  };

  const printFullReport = async () => {
    setIsPrintingFull(true);
    setError(null);
    try {
      await printFullReportThermal(startDate, endDate);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsPrintingFull(false);
    }
  };

  const currentViewTitle =
    view === "overview"
      ? "Overview (CSV)"
      : view === "products"
        ? "Product Wise Sales"
        : view === "tables"
          ? "Table Wise Sales"
          : "Refunds";

  // Table Wise Sales' thermal print needs `tables` enabled; Refunds' print
  // has no such module gate (refunds aren't a toggleable module).
  const canPrintCurrentView = view === "refunds" || (view === "tables" && tablesEnabled);

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex flex-wrap justify-end gap-2">
        <ActionCard
          title={currentViewTitle}
          icon={view === "overview" ? Download : FileText}
          onDownload={() => void downloadCurrentView()}
          isDownloading={disabled || isDownloadingCurrent}
          onPrint={canPrintCurrentView ? () => void printCurrentView() : undefined}
          isPrinting={isPrintingCurrentView}
        />

        <ActionCard
          title="Category Wise Sale"
          icon={FileText}
          onDownload={() => void downloadCategorySales()}
          isDownloading={disabled || isDownloadingCategory}
          onPrint={() => void printCategorySales()}
          isPrinting={isPrintingCategory}
        />

        <ActionCard
          title="Generate Full Report"
          icon={Layers}
          onDownload={() => void downloadFullReport()}
          isDownloading={disabled || isDownloadingFull}
          onPrint={() => void printFullReport()}
          isPrinting={isPrintingFull}
          accent
        />
      </div>
      {error && <p className="text-xs text-red-600">{error}</p>}
    </div>
  );
}
