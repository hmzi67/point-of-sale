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
import { runAction } from "../../utils/runAction";

type View = "overview" | "products" | "tables" | "refunds" | "delivery";

interface ExportButtonsProps {
  /** `null` on a tab that carries its own export (the profit, payment,
   * tax, void and inventory reports) — the tab-specific card is hidden. */
  view: View | null;
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
        "flex items-center gap-3 rounded-2xl border p-3",
        accent ? "border-accent/40 bg-accent-soft" : "border-line bg-surface",
      ].join(" ")}
    >
      <div
        className={[
          "flex h-10 w-10 shrink-0 items-center justify-center rounded-xl",
          accent ? "bg-accent text-accent-ink" : "bg-surface-3 text-fg-muted",
        ].join(" ")}
      >
        <Icon className="h-4 w-4" />
      </div>
      <div className="min-w-0">
        <p className={["text-[11px] font-semibold", accent ? "text-accent-text" : "text-fg-muted"].join(" ")}>
          {title}
        </p>
        <div className="mt-2 flex items-center gap-1.5">
          <button
            type="button"
            onClick={onDownload}
            disabled={isDownloading}
            className={[
              "flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50",
              accent ? "bg-accent text-accent-ink hover:bg-accent-hover" : "text-fg-2 hover:bg-surface-3",
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
                "flex items-center gap-1 rounded-lg px-2.5 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50",
                accent ? "text-accent-text hover:bg-accent-soft" : "text-fg-2 hover:bg-surface-3",
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

  /** One export action with its own busy flag, reporting into the shared
   * error line. `errorPrefix` labels downloads; thermal prints already
   * return a user-facing message. */
  const runExport = (setBusy: (busy: boolean) => void, action: () => Promise<void>, errorPrefix?: string) =>
    runAction(setBusy, setError, action, errorPrefix);

  // --- The active tab's own Download (+ Print, where one exists) --------
  const downloadCurrentView = () =>
    runExport(
      setIsDownloadingCurrent,
      async () => {
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
        } else if (view === "delivery") {
          const { getDeliveryReport } = await import("../../services/deliveryService");
          const report = await getDeliveryReport(startDate, endDate);
          const { buildCsv, downloadTextFile } = await import("../../utils/csv");
          const { minorToDecimal } = await import("../../utils/format");
          const rows = [
            ["Delivery Report", `${report.startDate} to ${report.endDate}`],
            ["Total Orders", report.totalOrders.toString()],
            ["In-House Orders", report.inHouseOrders.toString()],
            ["3rd-Party Orders", report.thirdPartyOrders.toString()],
            ["Total Sales", minorToDecimal(report.totalSalesMinor).toFixed(2)],
            ["Total Delivery Charges", minorToDecimal(report.totalDeliveryChargesMinor).toFixed(2)],
            [],
            ["Order #", "Type", "Status", "Platform", "Rider", "Customer", "Phone", "Address", "Delivery Charge", "Total"],
            ...report.orders.map((o) => [
              o.orderNo.toString(),
              o.orderType,
              o.deliveryStatus || "placed",
              o.platformName || "",
              o.riderName || "",
              o.customerName || "",
              o.customerPhone || "",
              o.deliveryAddress || "",
              minorToDecimal(o.deliveryChargeMinor).toFixed(2),
              minorToDecimal(o.totalMinor).toFixed(2),
            ]),
          ];
          await downloadTextFile(buildCsv(rows), `delivery-report-${startDate}-to-${endDate}.csv`);
        } else {
          const report = await getRefundsSummary(startDate, endDate);
          const { downloadRefundsSummaryPdf } = await import("../../utils/refundsSummaryPdf");
          await downloadRefundsSummaryPdf(report, config);
        }
      },
      "Could not prepare the download",
    );

  const printCurrentView = () =>
    runExport(setIsPrintingCurrentView, () =>
      view === "refunds" ? printRefundsSummaryThermal(startDate, endDate) : printTableSalesThermal(startDate, endDate),
    );

  // --- Category Wise Sale — no dedicated on-screen tab, so it's always
  // offered as its own card rather than folded into the tab-aware one. ---
  const downloadCategorySales = () =>
    runExport(
      setIsDownloadingCategory,
      async () => {
        const report = await getCategorySales(startDate, endDate);
        const { downloadCategorySalesPdf } = await import("../../utils/categorySalesPdf");
        await downloadCategorySalesPdf(report, config);
      },
      "Could not prepare the Category Wise Sale download",
    );

  const printCategorySales = () => runExport(setIsPrintingCategory, () => printCategorySalesThermal(startDate, endDate));

  // --- Overall Sales Report — consolidated sales summary document ---
  const downloadFullReport = () =>
    runExport(
      setIsDownloadingFull,
      async () => {
        const report = await getFullReport(startDate, endDate);
        const { downloadFullReportPdf } = await import("../../utils/fullReportPdf");
        await downloadFullReportPdf(report, config);
      },
      "Could not prepare the Overall Sales Report",
    );

  const printFullReport = () => runExport(setIsPrintingFull, () => printFullReportThermal(startDate, endDate));

  const currentViewTitle =
    view === "overview"
      ? "Overview (CSV)"
      : view === "products"
        ? "Product Wise Sales"
        : view === "tables"
          ? "Table Wise Sales"
          : view === "delivery"
            ? "Deliveries (CSV)"
            : "Refunds";

  // Table Wise Sales' thermal print needs `tables` enabled; Refunds' print
  // has no such module gate (refunds aren't a toggleable module).
  const canPrintCurrentView = view === "refunds" || (view === "tables" && tablesEnabled);

  return (
    <div className="flex flex-col items-end gap-1.5">
      <div className="flex flex-wrap justify-end gap-2">
        {view !== null && (
          <ActionCard
            title={currentViewTitle}
            icon={view === "overview" ? Download : FileText}
            onDownload={() => void downloadCurrentView()}
            isDownloading={disabled || isDownloadingCurrent}
            onPrint={canPrintCurrentView ? () => void printCurrentView() : undefined}
            isPrinting={isPrintingCurrentView}
          />
        )}

        <ActionCard
          title="Category Wise Sale"
          icon={FileText}
          onDownload={() => void downloadCategorySales()}
          isDownloading={disabled || isDownloadingCategory}
          onPrint={() => void printCategorySales()}
          isPrinting={isPrintingCategory}
        />

        <ActionCard
          title="Overall Sales Report"
          icon={Layers}
          onDownload={() => void downloadFullReport()}
          isDownloading={disabled || isDownloadingFull}
          onPrint={() => void printFullReport()}
          isPrinting={isPrintingFull}
          accent
        />
      </div>
      {error && <p className="text-xs text-danger-text">{error}</p>}
    </div>
  );
}
