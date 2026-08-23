import { lazy, Suspense, useEffect, useState } from "react";
import { DateRangePicker } from "../components/reports/DateRangePicker";
import { ExportButtons } from "../components/reports/ExportButtons";
import { ProductWiseSalesTable } from "../components/reports/ProductWiseSalesTable";
import { SummaryCards } from "../components/reports/SummaryCards";
import { TableWiseSalesTable } from "../components/reports/TableWiseSalesTable";
import { TopItemsTable } from "../components/reports/TopItemsTable";
import { useAppConfig } from "../hooks/useAppConfig";
import { useModules } from "../hooks/useModules";
import { getCategories } from "../services/inventoryService";
import { getProductSalesSummary, getTableSalesSummary } from "../services/reportsService";
import { useReportsStore } from "../store";
import type { Category, ProductSalesSummaryReport, TableSalesSummary, TopItemSort } from "../types";

// Recharts is large and only ever used here — deferred so it never loads on
// the (much more frequently visited) Billing/Dashboard/Inventory screens.
const SalesChart = lazy(() => import("../components/reports/SalesChart"));

type View = "overview" | "tables" | "products";

export function ReportsPage() {
  const { config } = useAppConfig();
  const { modules } = useModules();
  const tablesEnabled = modules.some((m) => m.key === "tables" && m.enabled);

  const summary = useReportsStore((state) => state.summary);
  const topItems = useReportsStore((state) => state.topItems);
  const series = useReportsStore((state) => state.series);
  const startDate = useReportsStore((state) => state.startDate);
  const endDate = useReportsStore((state) => state.endDate);
  const isLoading = useReportsStore((state) => state.isLoading);
  const error = useReportsStore((state) => state.error);
  const load = useReportsStore((state) => state.load);

  const [view, setView] = useState<View>("overview");

  // Table Wise Sales is its own, separate query — not folded into
  // useReportsStore's load(), since only the "tables" view ever needs it
  // and the Overview screen shouldn't wait on (or refetch for) data it
  // never shows. Falls back to "overview" if a client's Tables module gets
  // disabled while this tab happens to be selected.
  const [tableSales, setTableSales] = useState<TableSalesSummary | null>(null);
  const [isLoadingTableSales, setIsLoadingTableSales] = useState(false);
  const [tableSalesError, setTableSalesError] = useState<string | null>(null);

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

  useEffect(() => {
    // Runs once on mount — subsequent loads are triggered by the store's own
    // setters (setPreset/setCustomRange/setTopItemsSort), not by re-running this.
    void load();
  }, [load]);

  useEffect(() => {
    if (!tablesEnabled && view === "tables") setView("overview");
  }, [tablesEnabled, view]);

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
    void getCategories().then(setCategories);
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

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Reports</h2>
          <p className="text-sm text-slate-500">Daily sales summary and top-selling items.</p>
        </div>
        <ExportButtons
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

      <div className="flex items-center justify-between gap-3">
        <DateRangePicker />

        <div className="flex shrink-0 rounded-md border border-slate-300 p-0.5">
          {(["overview", "products", ...(tablesEnabled ? (["tables"] as const) : [])] as const).map((v) => (
            <button
              key={v}
              type="button"
              onClick={() => setView(v)}
              className={[
                "rounded px-3 py-1.5 text-sm font-medium transition-colors",
                view === v ? "bg-brand-600 text-white" : "text-slate-600 hover:bg-slate-100",
              ].join(" ")}
            >
              {v === "overview" ? "Overview" : v === "products" ? "Product Wise Sales" : "Table Wise Sales"}
            </button>
          ))}
        </div>
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      {view === "overview" ? (
        <>
          <SummaryCards summary={summary} currency={config.currency} isLoading={isLoading} />

          <Suspense fallback={<div className="h-[21rem] animate-pulse rounded-lg border border-slate-200 bg-slate-50" />}>
            <SalesChart series={series} currency={config.currency} isLoading={isLoading} />
          </Suspense>

          <TopItemsTable items={topItems} currency={config.currency} isLoading={isLoading} />
        </>
      ) : view === "products" ? (
        <>
          {productSalesError && (
            <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{productSalesError}</p>
          )}
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
        </>
      ) : (
        <>
          {tableSalesError && (
            <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{tableSalesError}</p>
          )}
          <TableWiseSalesTable data={tableSales} currency={config.currency} isLoading={isLoadingTableSales} />
        </>
      )}
    </section>
  );
}
