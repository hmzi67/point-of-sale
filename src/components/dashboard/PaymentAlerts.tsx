import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { AlertTriangle, CalendarClock } from "lucide-react";
import { getOverdueSummary } from "../../services/customersService";
import type { OverdueSummary } from "../../types";
import { formatMinor } from "../../utils/format";

/** Pay-later bills past (or close to) their due date. Renders nothing when
 * every bill is on time. */
export function PaymentAlerts({ currency }: { currency: string }) {
  const [summary, setSummary] = useState<OverdueSummary | null>(null);

  useEffect(() => {
    // An alert is a nicety on the landing page; a failed read just hides it.
    getOverdueSummary()
      .then(setSummary)
      .catch(() => setSummary(null));
  }, []);

  if (!summary || (summary.saleCount === 0 && summary.dueSoonCount === 0)) return null;
  const overdue = summary.saleCount > 0;

  return (
    <Link
      to="/customers"
      className={`flex flex-wrap items-center justify-between gap-3 rounded-2xl border p-4 ${overdue ? "border-danger-line bg-danger-soft text-danger-text" : "border-warning-line bg-warning-soft text-warning-text"}`}
    >
      <span className="flex items-center gap-2 text-sm font-semibold">
        {overdue ? <AlertTriangle className="h-4 w-4" /> : <CalendarClock className="h-4 w-4" />}
        {overdue
          ? `${summary.saleCount} pay-later bill(s) overdue from ${summary.customerCount} customer(s) — ${formatMinor(summary.amountMinor, currency)}`
          : `${summary.dueSoonCount} pay-later bill(s) due in the next 3 days`}
      </span>
      <span className="text-xs font-semibold underline">View customers</span>
    </Link>
  );
}
