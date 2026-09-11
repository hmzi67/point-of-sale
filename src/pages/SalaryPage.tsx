import { useEffect, useState } from "react";
import { Download, Printer } from "lucide-react";
import { MonthlyOverviewTable } from "../components/salary/MonthlyOverviewTable";
import { PaymentHistoryView } from "../components/salary/PaymentHistoryView";
import { RecordPaymentModal } from "../components/salary/RecordPaymentModal";
import { useAppConfig } from "../hooks/useAppConfig";
import { getMonthlyOverview, printMonthlyReportThermal } from "../services/salaryService";
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

  // "Employee Report" — attendance + salary for every active employee this
  // month, printable/downloadable straight from whatever `rows` the
  // Monthly overview tab is already showing, so the report always matches
  // what's on screen rather than re-querying separately.
  const [isDownloadingReport, setIsDownloadingReport] = useState(false);
  const [isPrintingReport, setIsPrintingReport] = useState(false);
  const [reportError, setReportError] = useState<string | null>(null);

  const downloadReport = async () => {
    setIsDownloadingReport(true);
    setReportError(null);
    try {
      const { downloadEmployeeReportPdf } = await import("../utils/employeeReportPdf");
      await downloadEmployeeReportPdf(rows, month, config);
    } catch (e) {
      setReportError((e as Error).message);
    } finally {
      setIsDownloadingReport(false);
    }
  };

  const printReport = async () => {
    setIsPrintingReport(true);
    setReportError(null);
    try {
      await printMonthlyReportThermal(month);
    } catch (e) {
      setReportError((e as Error).message);
    } finally {
      setIsPrintingReport(false);
    }
  };

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
        <h2 className="text-lg font-semibold text-fg">Salary</h2>
        <p className="text-sm text-fg-muted">
          Calculated from attendance — base salary ÷ days in that month × days present.
        </p>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex rounded-md border border-line-strong p-0.5 w-fit">
          <button
            type="button"
            onClick={() => setTab("overview")}
            className={[
              "rounded px-3 py-1.5 text-sm font-medium transition-colors",
              tab === "overview" ? "bg-accent text-accent-ink" : "text-fg-2 hover:bg-surface-3",
            ].join(" ")}
          >
            Monthly overview
          </button>
          <button
            type="button"
            onClick={() => setTab("history")}
            className={[
              "rounded px-3 py-1.5 text-sm font-medium transition-colors",
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
              value={month}
              onChange={(e) => setMonth(e.target.value)}
              className="rounded-md border border-line-strong px-2.5 py-1.5 text-sm"
            />
            <button
              type="button"
              onClick={() => void downloadReport()}
              disabled={isDownloadingReport || rows.length === 0}
              title="Download the Employee Report — attendance and salary for every employee this month"
              className="flex items-center gap-1.5 rounded-md border border-line-strong px-2.5 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Download className="h-3.5 w-3.5" />
              {isDownloadingReport ? "Preparing…" : "Employee Report"}
            </button>
            <button
              type="button"
              onClick={() => void printReport()}
              disabled={isPrintingReport || rows.length === 0}
              title="Print the Employee Report on the thermal printer"
              className="flex items-center gap-1.5 rounded-md border border-line-strong px-2.5 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Printer className="h-3.5 w-3.5" />
              {isPrintingReport ? "Printing…" : "Print"}
            </button>
          </div>
        )}
      </div>

      {reportError && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{reportError}</p>}
      {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

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
