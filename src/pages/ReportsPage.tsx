import { lazy, Suspense, useEffect } from "react";
import { DateRangePicker } from "../components/reports/DateRangePicker";
import { ExportButtons } from "../components/reports/ExportButtons";
import { SummaryCards } from "../components/reports/SummaryCards";
import { TopItemsTable } from "../components/reports/TopItemsTable";
import { useAppConfig } from "../hooks/useAppConfig";
import { useReportsStore } from "../store";

// Recharts is large and only ever used here — deferred so it never loads on
// the (much more frequently visited) Billing/Dashboard/Inventory screens.
const SalesChart = lazy(() => import("../components/reports/SalesChart"));

export function ReportsPage() {
  const { config } = useAppConfig();
  const summary = useReportsStore((state) => state.summary);
  const topItems = useReportsStore((state) => state.topItems);
  const series = useReportsStore((state) => state.series);
  const startDate = useReportsStore((state) => state.startDate);
  const endDate = useReportsStore((state) => state.endDate);
  const isLoading = useReportsStore((state) => state.isLoading);
  const error = useReportsStore((state) => state.error);
  const load = useReportsStore((state) => state.load);

  useEffect(() => {
    // Runs once on mount — subsequent loads are triggered by the store's own
    // setters (setPreset/setCustomRange/setTopItemsSort), not by re-running this.
    void load();
  }, [load]);

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
        />
      </div>

      <DateRangePicker />

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <SummaryCards summary={summary} currency={config.currency} isLoading={isLoading} />

      <Suspense fallback={<div className="h-[21rem] animate-pulse rounded-lg border border-slate-200 bg-slate-50" />}>
        <SalesChart series={series} currency={config.currency} isLoading={isLoading} />
      </Suspense>

      <TopItemsTable items={topItems} currency={config.currency} isLoading={isLoading} />
    </section>
  );
}
