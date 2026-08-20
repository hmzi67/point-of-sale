import { AlertTriangle, Receipt, RotateCcw, TrendingUp, Wallet } from "lucide-react";
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
    <div className="rounded-lg border border-slate-200 bg-white p-4">
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

/**
 * Today's snapshot: sales always shown (Billing is core, so it's never
 * missing), expenses only when that module is tracking anything (a `null`
 * total, not a `0`-value card — see `dashboard.rs`), and net profit computed
 * from whatever's actually enabled. A low-stock card only appears when
 * Inventory is on and there's something to flag.
 */
export function SnapshotCards({ summary, currency, isLoading }: SnapshotCardsProps) {
  const showExpenses = isLoading || summary?.totalExpensesMinor !== null;
  const showLowStock = !isLoading && summary !== null && summary.lowStockItemCount !== null && summary.lowStockItemCount > 0;
  // Only shown when there actually were refunds today — same "don't clutter
  // the snapshot with a card that's always zero" treatment as low stock.
  const showRefunds = !isLoading && summary !== null && summary.refundsMinor > 0;

  return (
    <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
      <Card
        icon={TrendingUp}
        label="Sales today"
        value={formatMinor(summary?.totalSalesMinor ?? 0, currency)}
        isLoading={isLoading}
      />
      {showExpenses && (
        <Card
          icon={Receipt}
          label="Expenses today"
          value={formatMinor(summary?.totalExpensesMinor ?? 0, currency)}
          isLoading={isLoading}
        />
      )}
      <Card
        icon={Wallet}
        label="Net profit today"
        value={formatMinor(summary?.netProfitMinor ?? 0, currency)}
        tone={!isLoading && summary && summary.netProfitMinor < 0 ? "negative" : "positive"}
        isLoading={isLoading}
      />
      {showRefunds && (
        <Card
          icon={RotateCcw}
          label="Refunds today"
          value={formatMinor(summary?.refundsMinor ?? 0, currency)}
          tone="negative"
          isLoading={isLoading}
        />
      )}
      {showLowStock && (
        <Card
          icon={AlertTriangle}
          label="Low stock items"
          value={String(summary?.lowStockItemCount ?? 0)}
          tone="negative"
          isLoading={isLoading}
        />
      )}
    </div>
  );
}
