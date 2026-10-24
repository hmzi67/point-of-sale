import {
  Ban,
  BarChart3,
  Bike,
  Boxes,
  CalendarDays,
  CreditCard,
  Landmark,
  Lock,
  PiggyBank,
  ReceiptText,
  RotateCcw,
  Table2,
  Monitor,
  Warehouse,
  type LucideIcon,
} from "lucide-react";
import { lazy, Suspense, useEffect, useState } from "react";
import { DateRangePicker } from "../components/reports/DateRangePicker";
import { DeliveryReport } from "../components/reports/DeliveryReport";
import { ExportButtons } from "../components/reports/ExportButtons";
import { InventoryReports } from "../components/reports/InventoryReports";
import { UpgradeNotice } from "../components/ui/UpgradeNotice";
import { useCapability } from "../hooks/useCapability";
import {
  OrderProfitReport,
  PaymentTypeReport,
  ProfitLossReport,
  TaxReportView,
  VoidReportView,
} from "../components/reports/SalesInsightReports";
import { WorstSellers } from "../components/reports/WorstSellers";
import { CounterSalesReport } from "../components/reports/CounterSalesReport";
import { ProductWiseSalesTable } from "../components/reports/ProductWiseSalesTable";
import { RefundsTable } from "../components/reports/RefundsTable";
import { SummaryCards } from "../components/reports/SummaryCards";
import { TableWiseSalesTable } from "../components/reports/TableWiseSalesTable";
import { TopItemsTable } from "../components/reports/TopItemsTable";
import { useAppConfig } from "../hooks/useAppConfig";
import { useModuleEnabled } from "../hooks/useModules";
import { getDashboardSummary } from "../services/dashboardService";
import { getDeliveryReport } from "../services/deliveryService";
import { getCategories } from "../services/inventoryService";
import { reportWarning } from "../services/logService";
import { getProductSalesSummary, getRefundsSummary, getTableSalesSummary } from "../services/reportsService";
import { useReportsStore } from "../store";
import type {
  Category,
  DashboardSummary,
  DeliveryReport as DeliveryReportData,
  ProductSalesSummaryReport,
  RefundsSummary,
  TableSalesSummary,
  TopItemSort,
} from "../types";

// Recharts is large and only ever used here — deferred so it never loads on
// the (much more frequently visited) Billing/Dashboard/Inventory screens.
const SalesChart = lazy(() => import("../components/reports/SalesChart").then((m) => ({ default: m.SalesChart })));
const TopEntriesChart = lazy(() =>
  import("../components/reports/TopEntriesChart").then((m) => ({ default: m.TopEntriesChart })),
);

type SalesView = "overview" | "counters" | "tables" | "products" | "refunds" | "delivery";
type InsightView = "profit" | "orderProfit" | "payments" | "tax" | "voids" | "inventory";
type View = SalesView | InsightView;

const TABS: { key: View; label: string; icon: LucideIcon }[] = [
  { key: "overview", label: "Overview", icon: BarChart3 },
  { key: "profit", label: "Profit & Loss", icon: PiggyBank },
  { key: "orderProfit", label: "Order Profit", icon: ReceiptText },
  { key: "products", label: "Product Wise Sales", icon: Boxes },
  { key: "tables", label: "Table Wise Sales", icon: Table2 },
  { key: "counters", label: "Counter Wise Sales", icon: Monitor },
  { key: "delivery", label: "Deliveries", icon: Bike },
  { key: "payments", label: "Payment Types", icon: CreditCard },
  { key: "tax", label: "Tax & Service", icon: Landmark },
  { key: "refunds", label: "Refunds", icon: RotateCcw },
  { key: "voids", label: "Voids & Returns", icon: Ban },
  { key: "inventory", label: "Inventory", icon: Warehouse },
];

const SALES_VIEWS: View[] = ["overview", "counters", "tables", "products", "refunds", "delivery"];

const chartFallback = <div className="h-[21rem] animate-pulse rounded-2xl border border-line bg-surface-2" />;

export function ReportsPage() {
  const { config } = useAppConfig();
  const tablesEnabled = useModuleEnabled("tables");
  const deliveryEnabled = useModuleEnabled("delivery");

  const summary = useReportsStore((state) => state.summary);
  const topItems = useReportsStore((state) => state.topItems);
  const series = useReportsStore((state) => state.series);
  const startDate = useReportsStore((state) => state.startDate);
  const endDate = useReportsStore((state) => state.endDate);
  const isLoading = useReportsStore((state) => state.isLoading);
  const error = useReportsStore((state) => state.error);
  const load = useReportsStore((state) => state.load);

  const [view, setView] = useState<View>("overview");
  // Profit, payment-type, tax, void and stock reports are Advanced Reports &
  // Analytics (a plan feature). The tabs stay visible, marked, and explain
  // the plan instead of failing; the commands refuse in Rust regardless.
  const advanced = useCapability("reports_advanced");
  const lockedInsight = !advanced.allowed && !SALES_VIEWS.includes(view);

  // Reuses the Dashboard's own net-profit math (`dashboard_get_summary`)
  // for whatever range Reports currently has selected, rather than this
  // screen recomputing profit a second way — see `SummaryCards`'s doc
  // comment. Its own query (not folded into `useReportsStore`) since only
  // the Overview card row ever needs it.
  const [profit, setProfit] = useState<DashboardSummary | null | undefined>(undefined);
  useEffect(() => {
    setProfit(undefined);
    getDashboardSummary(startDate, endDate)
      .then(setProfit)
      .catch(() => setProfit(null));
  }, [startDate, endDate]);

  // Table Wise Sales is its own, separate query — not folded into
  // useReportsStore's load(), since only the "tables" view ever needs it
  // and the Overview screen shouldn't wait on (or refetch for) data it
  // never shows. Falls back to "overview" if a client's Tables module gets
  // disabled while this tab happens to be selected.
  const [tableSales, setTableSales] = useState<TableSalesSummary | null>(null);
  const [isLoadingTableSales, setIsLoadingTableSales] = useState(false);
  const [tableSalesError, setTableSalesError] = useState<string | null>(null);

  // Delivery Report query — only fetched when "delivery" view is active
  // and the Delivery module is enabled.
  const [deliveryReport, setDeliveryReport] = useState<DeliveryReportData | null>(null);
  const [isLoadingDelivery, setIsLoadingDelivery] = useState(false);
  const [deliveryError, setDeliveryError] = useState<string | null>(null);

  // Product Wise Sales is likewise its own query, plus its own category
  // filter/sort controls (independent of Top Items' sort) and the category
  // list they're populated from — fetched once, lazily, the first time this
  // view is opened rather than on every Reports page load.
  const [categories, setCategories] = useState<Category[]>([]);
  const [productCategoryId, setProductCategoryId] = useState<number | null>(null);
  const [productSort, setProductSort] = useState<TopItemSort>("revenue");
  const [productSales, setProductSales] = useState<ProductSalesSummaryReport | null>(null);
  const [isLoadingProductSales, setIsLoadingProductSales] = useState(false);
  const [productSalesError, setProductSalesError] = useState<string | null>(null);

  // Refunds is likewise its own query — not folded into useReportsStore's
  // load(), same reasoning as Table Wise Sales/Product Wise Sales above.
  const [refundsSummary, setRefundsSummary] = useState<RefundsSummary | null>(null);
  const [isLoadingRefunds, setIsLoadingRefunds] = useState(false);
  const [refundsError, setRefundsError] = useState<string | null>(null);

  useEffect(() => {
    // Runs once on mount — subsequent loads are triggered by the store's own
    // setters (setPreset/setCustomRange/setTopItemsSort), not by re-running this.
    void load();
  }, [load]);

  useEffect(() => {
    if (!tablesEnabled && view === "tables") setView("overview");
  }, [tablesEnabled, view]);

  useEffect(() => {
    if (!deliveryEnabled && view === "delivery") setView("overview");
  }, [deliveryEnabled, view]);

  useEffect(() => {
    if (view !== "tables" || !tablesEnabled) return;
    setIsLoadingTableSales(true);
    setTableSalesError(null);
    getTableSalesSummary(startDate, endDate)
      .then(setTableSales)
      .catch((e: Error) => setTableSalesError(e.message))
      .finally(() => setIsLoadingTableSales(false));
  }, [view, tablesEnabled, startDate, endDate]);

  useEffect(() => {
    if (view !== "products" || categories.length > 0) return;
    // Only feeds the category filter — degrade to "no filter options".
    getCategories()
      .then(setCategories)
      .catch((error) => reportWarning(`[reports] could not load categories: ${(error as Error).message}`));
  }, [view, categories.length]);

  useEffect(() => {
    if (view !== "products") return;
    setIsLoadingProductSales(true);
    setProductSalesError(null);
    getProductSalesSummary(startDate, endDate, productCategoryId, productSort)
      .then(setProductSales)
      .catch((e: Error) => setProductSalesError(e.message))
      .finally(() => setIsLoadingProductSales(false));
  }, [view, startDate, endDate, productCategoryId, productSort]);

  useEffect(() => {
    if (view !== "delivery" || !deliveryEnabled) return;
    setIsLoadingDelivery(true);
    setDeliveryError(null);
    getDeliveryReport(startDate, endDate)
      .then(setDeliveryReport)
      .catch((e: Error) => setDeliveryError(e.message))
      .finally(() => setIsLoadingDelivery(false));
  }, [view, deliveryEnabled, startDate, endDate]);

  useEffect(() => {
    if (view !== "refunds") return;
    setIsLoadingRefunds(true);
    setRefundsError(null);
    getRefundsSummary(startDate, endDate)
      .then(setRefundsSummary)
      .catch((e: Error) => setRefundsError(e.message))
      .finally(() => setIsLoadingRefunds(false));
  }, [view, startDate, endDate]);

  return (
    <section className="space-y-6">
      <div className="flex flex-wrap items-start justify-between gap-5 py-1">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Business insights</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Reports</h2>
          <p className="mt-2 text-sm text-fg-muted">Explore your sales, spot your best sellers, and track performance.</p>
        </div>
        <ExportButtons
          // Counter Wise Sales has its own CSV button (see CounterSalesReport).
          view={SALES_VIEWS.includes(view) && view !== "counters" ? (view as Exclude<SalesView, "counters">) : null}
          summary={summary}
          topItems={topItems}
          series={series}
          config={config}
          startDate={startDate}
          endDate={endDate}
          tablesEnabled={tablesEnabled}
          productCategoryId={productCategoryId}
          productSort={productSort}
        />
      </div>

      <div className="space-y-4 rounded-2xl border border-line bg-surface p-4 sm:p-5">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <span className="flex items-center gap-2 text-xs font-semibold text-fg-2"><CalendarDays className="h-4 w-4 text-accent-text" />Reporting period</span>
          <DateRangePicker />
        </div>

        <div className="flex flex-wrap gap-2 border-t border-line pt-4">
          {TABS.filter((t) => (t.key !== "tables" || tablesEnabled) && (t.key !== "delivery" || deliveryEnabled)).map(({ key, label, icon: Icon }) => (
            <button
              key={key}
              type="button"
              onClick={() => setView(key)}
              aria-pressed={view === key}
              className={[
                "flex items-center gap-2 rounded-xl border px-4 py-2.5 text-xs font-semibold transition-colors",
                view === key ? "border-accent/40 bg-accent-soft text-accent-text" : "border-transparent text-fg-muted hover:bg-surface-2",
              ].join(" ")}
            >
              <Icon className="h-4 w-4" />
              {label}
              {!advanced.allowed && !SALES_VIEWS.includes(key) && <Lock className="h-3 w-3 opacity-70" aria-label="Not in your plan" />}
            </button>
          ))}
        </div>
      </div>

      {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      {lockedInsight ? (
        <UpgradeNotice feature={TABS.find((t) => t.key === view)?.label ?? "This report"} requiredPlan={advanced.requiredPlan} />
      ) : view === "overview" ? (
        <>
          <SummaryCards summary={summary} currency={config.currency} isLoading={isLoading} profit={profit} />

          <Suspense fallback={chartFallback}>
            <SalesChart series={series} currency={config.currency} isLoading={isLoading} />
          </Suspense>

          <TopItemsTable items={topItems} currency={config.currency} isLoading={isLoading} />
        </>
      ) : view === "products" ? (
        <>
          {productSalesError && (
            <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{productSalesError}</p>
          )}
          <Suspense fallback={chartFallback}>
            <TopEntriesChart
              title="Top products by revenue"
              entries={(productSales?.rows ?? []).map((row) => ({ label: row.itemName, valueMinor: row.revenueMinor }))}
              currency={config.currency}
              isLoading={isLoadingProductSales}
            />
          </Suspense>
          <ProductWiseSalesTable
            data={productSales}
            categories={categories}
            categoryId={productCategoryId}
            onCategoryChange={setProductCategoryId}
            sortBy={productSort}
            onSortChange={setProductSort}
            currency={config.currency}
            isLoading={isLoadingProductSales}
          />
          <WorstSellers data={productSales} currency={config.currency} />
        </>
      ) : view === "tables" ? (
        <>
          {tableSalesError && (
            <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{tableSalesError}</p>
          )}
          <Suspense fallback={chartFallback}>
            <TopEntriesChart
              title="Top tables by sales"
              entries={(tableSales?.rows ?? []).map((row) => ({ label: row.label, valueMinor: row.totalMinor }))}
              currency={config.currency}
              isLoading={isLoadingTableSales}
            />
          </Suspense>
          <TableWiseSalesTable data={tableSales} currency={config.currency} isLoading={isLoadingTableSales} />
        </>
      ) : view === "counters" ? (
        <CounterSalesReport startDate={startDate} endDate={endDate} currency={config.currency} />
      ) : view === "profit" ? (
        <ProfitLossReport startDate={startDate} endDate={endDate} currency={config.currency} />
      ) : view === "orderProfit" ? (
        <OrderProfitReport startDate={startDate} endDate={endDate} currency={config.currency} />
      ) : view === "payments" ? (
        <PaymentTypeReport startDate={startDate} endDate={endDate} currency={config.currency} />
      ) : view === "tax" ? (
        <TaxReportView startDate={startDate} endDate={endDate} currency={config.currency} />
      ) : view === "voids" ? (
        <VoidReportView startDate={startDate} endDate={endDate} currency={config.currency} />
      ) : view === "inventory" ? (
        <InventoryReports startDate={startDate} endDate={endDate} currency={config.currency} />
      ) : view === "delivery" ? (
        <>
          {deliveryError && (
            <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{deliveryError}</p>
          )}
          <DeliveryReport data={deliveryReport} currency={config.currency} isLoading={isLoadingDelivery} />
        </>
      ) : (
        <>
          {refundsError && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{refundsError}</p>}
          <RefundsTable data={refundsSummary} currency={config.currency} isLoading={isLoadingRefunds} />
        </>
      )}
    </section>
  );
}
