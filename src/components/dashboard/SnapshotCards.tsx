import { AlertTriangle, Bike, Receipt, RotateCcw, Table2, TrendingUp, Wallet } from "lucide-react";
import { formatMinor } from "../../utils/format";
import type { DashboardSummary } from "../../types";

interface SnapshotCardsProps {
  summary: DashboardSummary | null;
  currency: string;
  isLoading: boolean;
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
    <div className={`relative overflow-hidden rounded-2xl border p-5 ${featured ? "border-accent/30 bg-linear-to-br from-accent-soft via-surface to-surface" : "border-line bg-surface glass"}`}>
      {featured && <div aria-hidden="true" className="pointer-events-none absolute -right-6 -top-6 h-28 w-28 rounded-full border-[16px] border-accent/5" />}
      <div className="relative flex items-center justify-between gap-3">
        <span className="text-xs font-medium text-fg-muted">{label}</span>
        <span className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-xl ${tone === "negative" ? "bg-danger-soft text-danger-text" : featured || tone === "positive" ? "bg-accent-soft text-accent-text" : "bg-surface-3 text-fg-2"}`}>
          <Icon className="h-5 w-5" strokeWidth={1.75} />
        </span>
      </div>
      <p className={`relative mt-4 break-words text-2xl font-bold tracking-tight tabular-nums ${valueColor}`}>
        {isLoading ? <span className="inline-block h-8 w-32 animate-pulse rounded bg-surface-3" /> : value}
      </p>
      <div className="mt-4 border-t border-line/70 pt-3 text-xs text-fg-muted">
        {isLoading ? <span className="block h-3 w-24 animate-pulse rounded bg-surface-3" /> : detail}
      </div>
    </div>
  );
}

/**
 * Today's snapshot: sales always shown (Billing is core, so it's never
 * missing), expenses only when that module is tracking anything (a `null`
 * total, not a `0`-value card — see `dashboard.rs`), and net profit computed
 * from whatever's actually enabled. A low-stock card only appears when
 * Inventory is on and there's something to flag.
 */
export function SnapshotCards({ summary, currency, isLoading }: SnapshotCardsProps) {
  const showExpenses = isLoading || (summary != null && summary.totalExpensesMinor !== null);
  const showLowStock = !isLoading && summary !== null && summary.lowStockItemCount !== null && summary.lowStockItemCount > 0;
  // Only shown when there actually were refunds today — same "don't clutter
  // the snapshot with a card that's always zero" treatment as low stock.
  const showRefunds = !isLoading && summary !== null && summary.refundsMinor > 0;
  // `topTableBySales` is already `null` when the `tables` module is
  // disabled *or* when it's enabled but nothing was sold to a table today —
  // see `dashboard.rs` — so this card simply mirrors whatever it says,
  // same pattern as every other optional card here.
  const showTopTable = isLoading || summary?.topTableBySales != null;
  const showDelivery = isLoading || summary?.deliveryStats != null;

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <Card
        icon={TrendingUp}
        label="Sales today"
        featured
        detail={`${summary?.transactionCount ?? 0} completed transaction${summary?.transactionCount === 1 ? "" : "s"}`}
        value={formatMinor(summary?.totalSalesMinor ?? 0, currency)}
        isLoading={isLoading}
      />
      {showExpenses && (
        <Card
          icon={Receipt}
          label="Expenses today"
          detail="Recorded expenses today"
          value={formatMinor(summary?.totalExpensesMinor ?? 0, currency)}
          isLoading={isLoading}
        />
      )}
      <Card
        icon={Wallet}
        label="Net profit today"
        detail="After refunds and tracked costs"
        value={formatMinor(summary?.netProfitMinor ?? 0, currency)}
        tone={!isLoading && summary && summary.netProfitMinor < 0 ? "negative" : "positive"}
        isLoading={isLoading}
      />
      {showRefunds && (
        <Card
          icon={RotateCcw}
          label="Refunds today"
          detail="Refunds issued today"
          value={formatMinor(summary?.refundsMinor ?? 0, currency)}
          tone="negative"
          isLoading={isLoading}
        />
      )}
      {showTopTable && (
        <Card
          icon={Table2}
          label="Top table by sales"
          detail="Highest table sales today"
          value={
            summary?.topTableBySales
              ? `${summary.topTableBySales.name} · ${formatMinor(summary.topTableBySales.totalMinor, currency)}`
              : "—"
          }
          isLoading={isLoading}
        />
      )}
      {showDelivery && (
        <Card
          icon={Bike}
          label="Deliveries today"
          detail={
            summary?.deliveryStats
              ? `${summary.deliveryStats.orderCount} delivery order${summary.deliveryStats.orderCount === 1 ? "" : "s"}`
              : "No deliveries today"
          }
          value={formatMinor(summary?.deliveryStats?.salesMinor ?? 0, currency)}
          isLoading={isLoading}
        />
      )}
      {showLowStock && (
        <Card
          icon={AlertTriangle}
          label="Low stock items"
          detail="Items at or below their stock threshold"
          value={String(summary?.lowStockItemCount ?? 0)}
          tone="negative"
          isLoading={isLoading}
        />
      )}
    </div>
  );
}
