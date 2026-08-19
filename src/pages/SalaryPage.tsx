import { useEffect, useState } from "react";
import { MonthlyOverviewTable } from "../components/salary/MonthlyOverviewTable";
import { PaymentHistoryView } from "../components/salary/PaymentHistoryView";
import { RecordPaymentModal } from "../components/salary/RecordPaymentModal";
import { useAppConfig } from "../hooks/useAppConfig";
import { getMonthlyOverview } from "../services/salaryService";
import type { SalaryCalculation } from "../types";

type Tab = "overview" | "history";

function currentMonth(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
}

export function SalaryPage() {
  const { config } = useAppConfig();
  const [tab, setTab] = useState<Tab>("overview");
  const [month, setMonth] = useState(currentMonth());
  const [rows, setRows] = useState<SalaryCalculation[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [paymentTarget, setPaymentTarget] = useState<SalaryCalculation | null>(null);

  const reload = () => {
    setIsLoading(true);
    setError(null);
    getMonthlyOverview(month)
      .then(setRows)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  };

  useEffect(reload, [month]);

  return (
    <section className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold text-slate-900">Salary</h2>
        <p className="text-sm text-slate-500">
          Calculated from attendance — {config.workingDaysPerMonth} working days a month.
        </p>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex rounded-md border border-slate-300 p-0.5 w-fit">
          <button
            type="button"
            onClick={() => setTab("overview")}
            className={[
              "rounded px-3 py-1.5 text-sm font-medium transition-colors",
              tab === "overview" ? "bg-brand-600 text-white" : "text-slate-600 hover:bg-slate-100",
            ].join(" ")}
          >
            Monthly overview
          </button>
          <button
            type="button"
            onClick={() => setTab("history")}
            className={[
              "rounded px-3 py-1.5 text-sm font-medium transition-colors",
              tab === "history" ? "bg-brand-600 text-white" : "text-slate-600 hover:bg-slate-100",
            ].join(" ")}
          >
            Payment history
          </button>
        </div>

        {tab === "overview" && (
          <input
            type="month"
            value={month}
            onChange={(e) => setMonth(e.target.value)}
            className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
          />
        )}
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      {tab === "overview" ? (
        <MonthlyOverviewTable
          rows={rows}
          currency={config.currency}
          isLoading={isLoading}
          onRecordPayment={setPaymentTarget}
        />
      ) : (
        <PaymentHistoryView />
      )}

      {paymentTarget && (
        <RecordPaymentModal
          row={paymentTarget}
          currency={config.currency}
          onClose={() => setPaymentTarget(null)}
          onSaved={(updated) => {
            setRows((prev) => prev.map((r) => (r.employeeId === updated.employeeId ? updated : r)));
            setPaymentTarget(null);
          }}
        />
      )}
    </section>
  );
}
