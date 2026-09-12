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
  detail,
  featured = false,
}: {
  detail: string;
  featured?: boolean;
  icon: typeof Wallet;
  label: string;
  value: string;
  tone?: "default" | "positive" | "negative";
  isLoading: boolean;
}) {
  const valueColor =
    tone === "positive" ? "text-success-text" : tone === "negative" ? "text-danger-text" : "text-fg";

  return (
    <div className={`rounded-2xl border p-5 ${featured ? "border-accent/25 bg-linear-to-br from-accent-soft to-surface" : "border-line bg-surface"}`}>
      <div className="flex items-center justify-between gap-3">
        <span className="text-xs font-medium text-fg-muted">{label}</span>
        <span className={`flex h-10 w-10 items-center justify-center rounded-xl ${tone === "negative" ? "bg-danger-soft text-danger-text" : featured || tone === "positive" ? "bg-accent-soft text-accent-text" : "bg-surface-3 text-fg-2"}`}><Icon className="h-5 w-5" strokeWidth={1.75} /></span>
      </div>
      <p className={`mt-4 break-words text-2xl font-bold tracking-tight tabular-nums ${valueColor}`}>
        {isLoading ? <span className="inline-block h-8 w-32 animate-pulse rounded bg-surface-3" /> : value}
      </p>
      <p className="mt-4 border-t border-line/70 pt-3 text-[11px] text-fg-muted">{detail}</p>
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
    <div className={`grid grid-cols-1 sm:grid-cols-2 gap-4 ${showProfit ? "lg:grid-cols-4" : "lg:grid-cols-3"}`}>
      <Card
        icon={Wallet}
        label="Total sales"
        featured
        detail="Sales in the selected period"
        value={formatMinor(summary?.totalSalesMinor ?? 0, currency)}
        isLoading={isLoading}
      />
      <Card icon={Hash} label="Transactions" detail="Completed sales in this period" value={String(summary?.transactionCount ?? 0)} isLoading={isLoading} />
      <Card
        icon={TrendingUp}
        label="Average sale"
        detail="Average value per transaction"
        value={formatMinor(summary?.averageSaleMinor ?? 0, currency)}
        isLoading={isLoading}
      />
      {showProfit && (
        <Card
          icon={TrendingUp}
          label="Net profit"
          detail="After refunds and tracked costs"
          value={formatMinor(profit?.netProfitMinor ?? 0, currency)}
          tone={!isLoading && profit && profit.netProfitMinor < 0 ? "negative" : "positive"}
          isLoading={isLoading}
        />
      )}
    </div>
  );
}
