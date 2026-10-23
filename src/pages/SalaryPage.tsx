import { ManagementSummary } from "../components/ui/ManagementSummary";
import { formatMinor } from "../utils/format";
import { useState } from "react";
import { Download, Printer, Wallet, Banknote, Clock } from "lucide-react";
import { MonthlyOverviewTable } from "../components/salary/MonthlyOverviewTable";
import { PaymentHistoryView } from "../components/salary/PaymentHistoryView";
import { RecordPaymentModal } from "../components/salary/RecordPaymentModal";
import { useAppConfig } from "../hooks/useAppConfig";
import { useReport } from "../hooks/useReport";
import { getMonthlyOverview, printMonthlyReportThermal } from "../services/salaryService";
import type { SalaryCalculation } from "../types";
import { currentMonth } from "../utils/dateRanges";
import { runAction } from "../utils/runAction";

type Tab = "overview" | "history";

export function SalaryPage() {
  const { config } = useAppConfig();
  const [tab, setTab] = useState<Tab>("overview");
  const [month, setMonth] = useState(currentMonth());
  const [paymentTarget, setPaymentTarget] = useState<SalaryCalculation | null>(null);

  // "Employee Report" — attendance + salary for every active employee this
  // month, printable/downloadable straight from whatever `rows` the
  // Monthly overview tab is already showing, so the report always matches
  // what's on screen rather than re-querying separately.
  const [isDownloadingReport, setIsDownloadingReport] = useState(false);
  const [isPrintingReport, setIsPrintingReport] = useState(false);
  const [reportError, setReportError] = useState<string | null>(null);

  const downloadReport = async () => {
    await runAction(setIsDownloadingReport, setReportError, async () => {
      const { downloadEmployeeReportPdf } = await import("../utils/employeeReportPdf");
      await downloadEmployeeReportPdf(rows, month, config);
    });
  };

  const printReport = async () => {
    await runAction(setIsPrintingReport, setReportError, async () => {
      await printMonthlyReportThermal(month);
    });
  };

  const overview = useReport(() => getMonthlyOverview(month), [month]);
  const rows: SalaryCalculation[] = overview.data ?? [];
  const { isLoading, error } = overview;

  return (
    <section className="space-y-6">
      <div>
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Payroll overview</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Salary</h2>
        <p className="mt-2 max-w-2xl text-sm leading-relaxed text-fg-muted">
          Calculated from attendance — base salary ÷ days in that month × days present.
        </p>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-line bg-surface p-4">
        <div className="flex flex-wrap gap-1 rounded-2xl border border-line bg-surface p-1.5 w-fit">
          <button
            type="button"
            onClick={() => setTab("overview")}
            aria-pressed={tab === "overview"}
            className={[
              "rounded-lg px-4 py-2.5 text-sm font-medium transition-colors",
              tab === "overview" ? "bg-accent text-accent-ink" : "text-fg-2 hover:bg-surface-3",
            ].join(" ")}
          >
            Monthly overview
          </button>
          <button
            type="button"
            onClick={() => setTab("history")}
            aria-pressed={tab === "history"}
            className={[
              "rounded-lg px-4 py-2.5 text-sm font-medium transition-colors",
              tab === "history" ? "bg-accent text-accent-ink" : "text-fg-2 hover:bg-surface-3",
            ].join(" ")}
          >
            Payment history
          </button>
        </div>

        {tab === "overview" && (
          <div className="flex flex-wrap items-center gap-2">
            <input
              type="month"
              aria-label="Payroll month"
              value={month}
              onChange={(e) => setMonth(e.target.value)}
              className="rounded-xl border border-line-strong px-3 py-2.5 text-sm"
            />
            <button
              type="button"
              onClick={() => void downloadReport()}
              disabled={isDownloadingReport || rows.length === 0}
              title="Download the Employee Report — attendance and salary for every employee this month"
              className="flex items-center gap-1.5 rounded-xl border border-line-strong px-3 py-2.5 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Download className="h-3.5 w-3.5" />
              {isDownloadingReport ? "Preparing…" : "Employee Report"}
            </button>
            <button
              type="button"
              onClick={() => void printReport()}
              disabled={isPrintingReport || rows.length === 0}
              title="Print the Employee Report on the thermal printer"
              className="flex items-center gap-1.5 rounded-xl border border-line-strong px-3 py-2.5 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Printer className="h-3.5 w-3.5" />
              {isPrintingReport ? "Printing…" : "Print"}
            </button>
          </div>
        )}
      </div>

      {tab === "overview" && <ManagementSummary isLoading={isLoading} items={[
        { label: "Calculated payroll", value: formatMinor(rows.reduce((sum, row) => sum + row.calculatedAmountMinor, 0), config.currency), detail: "Attendance-based pay for the selected month", icon: Wallet },
        { label: "Paid this month", value: formatMinor(rows.reduce((sum, row) => sum + row.paidAmountMinor, 0), config.currency), detail: "Payments against this month's payroll", icon: Banknote, tone: "success" },
        { label: "Awaiting payment", value: rows.filter((row) => row.status !== "paid").length, detail: "Employees marked unpaid or partially paid", icon: Clock, tone: "warning" },
      ]} />}

      {reportError && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{reportError}</p>}
      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

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
          onSaved={() => {
            overview.reload();
            setPaymentTarget(null);
          }}
        />
      )}
    </section>
  );
}
