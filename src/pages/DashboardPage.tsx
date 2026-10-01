import { lazy, Suspense, useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ArrowUpRight, CalendarDays } from "lucide-react";
import { QuickLinks } from "../components/dashboard/QuickLinks";
import { SnapshotCards } from "../components/dashboard/SnapshotCards";
import { useAppConfig } from "../hooks/useAppConfig";
import { useModules } from "../hooks/useModules";
import { getDashboardSummary } from "../services/dashboardService";
import { getSalesOverTime } from "../services/reportsService";
import { resolveDateRangePreset } from "../utils/dateRanges";
import type { DailySales, DashboardSummary } from "../types";

// Recharts is large and only used by the trend chart — deferred so it never
// loads before the rest of the landing page has painted.
const MonthlyTrendChart = lazy(() =>
  import("../components/dashboard/MonthlyTrendChart").then((m) => ({ default: m.MonthlyTrendChart })),
);

/** The default landing page for owner/admin. Aggregates Billing, Expenses
 * and Salary — the modules Phase 1's system may or may not have enabled for
 * this client — into one glance at "how's the shop doing". */
export function DashboardPage() {
  const { config } = useAppConfig();
  const { visibleModules, canAccess } = useModules();

  const [summary, setSummary] = useState<DashboardSummary | null>(null);
  const [series, setSeries] = useState<DailySales[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const today = resolveDateRangePreset("today");
    const monthToDate = resolveDateRangePreset("thisMonth");

    setIsLoading(true);
    setError(null);
    Promise.all([
      getDashboardSummary(today.startDate, today.endDate),
      getSalesOverTime(monthToDate.startDate, monthToDate.endDate),
    ])
      .then(([todaySummary, monthSeries]) => {
        setSummary(todaySummary);
        setSeries(monthSeries);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  }, []);

  return (
    <section className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-4 py-1">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Store overview</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Dashboard</h2>
          <p className="mt-2 text-sm text-fg-muted">A clear view of your business, at a glance.</p>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <span className="flex items-center gap-2 rounded-xl border border-line bg-surface px-3.5 py-2.5 text-xs font-medium text-fg-2">
            <CalendarDays className="h-4 w-4 text-fg-muted" />
            {new Intl.DateTimeFormat(undefined, { weekday: "short", day: "numeric", month: "short", year: "numeric" }).format(new Date())}
          </span>
          {canAccess("billing") && (
            <Link to="/billing" className="flex items-center gap-2 rounded-xl bg-accent px-4 py-2.5 text-sm font-semibold text-accent-ink transition-colors hover:bg-accent-hover">
              Open billing <ArrowUpRight className="h-4 w-4" />
            </Link>
          )}
        </div>
      </div>

      {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <SnapshotCards summary={summary} currency={config.currency} isLoading={isLoading} />

      <div className="grid items-start gap-6 xl:grid-cols-[minmax(0,1fr)_340px] 2xl:grid-cols-[minmax(0,1fr)_380px]">
        <Suspense fallback={<div className="h-[30rem] animate-pulse rounded-2xl border border-line bg-surface-2" />}>
          <MonthlyTrendChart series={series} currency={config.currency} isLoading={isLoading} />
        </Suspense>
        <QuickLinks modules={visibleModules} />
      </div>
    </section>
  );
}
