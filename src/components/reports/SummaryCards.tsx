import { Hash, TrendingUp, Wallet } from "lucide-react";
import { formatMinor } from "../../utils/format";
import type { DashboardSummary, SalesSummary } from "../../types";

interface SummaryCardsProps {
  summary: SalesSummary | null;
  currency: string;
  isLoading: boolean;
  /** Reuses the Dashboard's own aggregate (`net_profit_minor` and friends)
   * for the range currently selected here — see `dashboard.rs` — rather
   * than this screen recomputing profit a second way. `undefined` while
   * still loading, `null` if the fetch failed; either way the card is
   * simply not shown, same "optional module, optional card" treatment
   * `SnapshotCards` gives expenses/salary. */
  profit?: DashboardSummary | null;
}

function Card({
  icon: Icon,
  label,
  value,
  tone = "default",
  isLoading,
}: {
  icon: typeof Wallet;
  label: string;
  value: string;
  tone?: "default" | "positive" | "negative";
  isLoading: boolean;
}) {
  const valueColor =
    tone === "positive" ? "text-emerald-600" : tone === "negative" ? "text-red-600" : "text-slate-900";

  return (
    <div className="rounded-2xl border border-slate-200 bg-white p-4 shadow-soft">
      <div className="flex items-center gap-2 text-slate-500">
        <Icon className="h-4 w-4" />
        <span className="text-xs font-medium uppercase tracking-wide">{label}</span>
      </div>
      <p className={`mt-2 text-2xl font-bold ${valueColor}`}>
        {isLoading ? <span className="inline-block h-7 w-24 animate-pulse rounded bg-slate-100" /> : value}
      </p>
    </div>
  );
}

export function SummaryCards({ summary, currency, isLoading, profit }: SummaryCardsProps) {
  // `profit` is only ever `null` once a fetch has actually resolved and
  // come back empty — while it's still `undefined` (not fetched/loading
  // yet), showLoading covers the card the same way it covers the other
  // three, so it doesn't flash in a beat after them.
  const showProfit = isLoading || profit !== null;

  return (
    <div className={`grid grid-cols-2 gap-4 ${showProfit ? "lg:grid-cols-4" : "lg:grid-cols-3"}`}>
      <Card
        icon={Wallet}
        label="Total sales"
        value={formatMinor(summary?.totalSalesMinor ?? 0, currency)}
        isLoading={isLoading}
      />
      <Card icon={Hash} label="Transactions" value={String(summary?.transactionCount ?? 0)} isLoading={isLoading} />
      <Card
        icon={TrendingUp}
        label="Average sale"
        value={formatMinor(summary?.averageSaleMinor ?? 0, currency)}
        isLoading={isLoading}
      />
      {showProfit && (
        <Card
          icon={TrendingUp}
          label="Net profit"
          value={formatMinor(profit?.netProfitMinor ?? 0, currency)}
          tone={!isLoading && profit && profit.netProfitMinor < 0 ? "negative" : "positive"}
          isLoading={isLoading}
        />
      )}
    </div>
  );
}
