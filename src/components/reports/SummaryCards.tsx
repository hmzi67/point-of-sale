import { Hash, TrendingUp, Wallet } from "lucide-react";
import { formatMinor } from "../../utils/format";
import type { SalesSummary } from "../../types";

interface SummaryCardsProps {
  summary: SalesSummary | null;
  currency: string;
  isLoading: boolean;
}

function Card({
  icon: Icon,
  label,
  value,
  isLoading,
}: {
  icon: typeof Wallet;
  label: string;
  value: string;
  isLoading: boolean;
}) {
  return (
    <div className="rounded-lg border border-slate-200 bg-white p-4">
      <div className="flex items-center gap-2 text-slate-500">
        <Icon className="h-4 w-4" />
        <span className="text-xs font-medium uppercase tracking-wide">{label}</span>
      </div>
      <p className="mt-2 text-2xl font-bold text-slate-900">
        {isLoading ? <span className="inline-block h-7 w-24 animate-pulse rounded bg-slate-100" /> : value}
      </p>
    </div>
  );
}

export function SummaryCards({ summary, currency, isLoading }: SummaryCardsProps) {
  return (
    <div className="grid grid-cols-3 gap-4">
      <Card
        icon={Wallet}
        label="Total sales"
        value={formatMinor(summary?.totalSalesMinor ?? 0, currency)}
        isLoading={isLoading}
      />
      <Card
        icon={Hash}
        label="Transactions"
        value={String(summary?.transactionCount ?? 0)}
        isLoading={isLoading}
      />
      <Card
        icon={TrendingUp}
        label="Average sale"
        value={formatMinor(summary?.averageSaleMinor ?? 0, currency)}
        isLoading={isLoading}
      />
    </div>
  );
}
