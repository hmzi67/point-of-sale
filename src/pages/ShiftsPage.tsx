import { useEffect, useState } from "react";
import { Download, Printer } from "lucide-react";
import { useAppConfig } from "../hooks/useAppConfig";
import { getShiftSummary, listRecentShifts, printShiftSummaryThermal } from "../services/shiftsService";
import { formatMinor } from "../utils/format";
import type { Shift, ShiftSummary } from "../types";

/** Shift history — Owner/Admin (see MODULE_CATALOGUE's doc comment on the
 * `shifts` module). Opening/closing a shift itself happens inline on the
 * Billing screen; this page is purely "what happened on past shifts",
 * mirroring the read-only role Reports plays for sales. */
export function ShiftsPage() {
  const { config } = useAppConfig();
  const [shifts, setShifts] = useState<Shift[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [openId, setOpenId] = useState<number | null>(null);
  const [summary, setSummary] = useState<ShiftSummary | null>(null);
  const [printStatus, setPrintStatus] = useState<string | null>(null);

  useEffect(() => {
    listRecentShifts(100)
      .then(setShifts)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  }, []);

  const toggle = async (shift: Shift) => {
    if (openId === shift.id) {
      setOpenId(null);
      setSummary(null);
      return;
    }
    setOpenId(shift.id);
    setSummary(null);
    setPrintStatus(null);
    try {
      const s = await getShiftSummary(shift.id, null);
      setSummary(s);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const printThermal = async (shiftId: number) => {
    setPrintStatus(null);
    try {
      await printShiftSummaryThermal(shiftId);
      setPrintStatus("Sent to thermal printer.");
    } catch (e) {
      setPrintStatus((e as Error).message);
    }
  };

  const downloadPdf = async () => {
    if (!summary) return;
    try {
      const { downloadShiftSummaryPdf } = await import("../utils/shiftSummaryPdf");
      downloadShiftSummaryPdf(summary, config);
    } catch (e) {
      setPrintStatus(`Could not prepare the PDF: ${(e as Error).message}`);
    }
  };

  return (
    <section className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold text-slate-900">Shifts</h2>
        <p className="text-sm text-slate-500">Every cashier shift, opening balance, and close-out reconciliation.</p>
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      {isLoading ? (
        <div className="h-40 animate-pulse rounded-lg border border-slate-200 bg-slate-50" />
      ) : shifts.length === 0 ? (
        <p className="rounded-lg border border-dashed border-slate-300 py-10 text-center text-sm text-slate-400">
          No shifts opened yet.
        </p>
      ) : (
        <div className="divide-y divide-slate-200 rounded-lg border border-slate-200 bg-white">
          {shifts.map((shift) => (
            <div key={shift.id}>
              <button
                type="button"
                onClick={() => void toggle(shift)}
                className="flex w-full items-center justify-between gap-3 px-4 py-3 text-left text-sm hover:bg-slate-50"
              >
                <span>
                  <span className="block font-medium text-slate-900">
                    Shift #{shift.id} · {shift.cashierName ?? "—"}
                  </span>
                  <span className="block text-xs text-slate-500">
                    {shift.openedAt} {shift.closedAt ? `– ${shift.closedAt}` : "(open)"}
                  </span>
                </span>
                <span
                  className={[
                    "shrink-0 rounded-full px-2.5 py-1 text-xs font-semibold",
                    shift.closedAt ? "bg-slate-100 text-slate-600" : "bg-emerald-50 text-emerald-600",
                  ].join(" ")}
                >
                  {shift.closedAt ? "Closed" : "Open"}
                </span>
              </button>

              {openId === shift.id && summary && (
                <div className="space-y-2 border-t border-slate-100 bg-slate-50 px-4 py-3 text-sm">
                  <Row label="Opening Balance" value={formatMinor(summary.openingBalanceMinor, config.currency)} />
                  <Row label="Cash Sale" value={formatMinor(summary.cashSalesMinor, config.currency)} />
                  <Row label="Card Sale" value={formatMinor(summary.cardSalesMinor, config.currency)} />
                  <Row label="Other Sale" value={formatMinor(summary.otherSalesMinor, config.currency)} />
                  <Row label="Refund Today" value={formatMinor(summary.refundsMinor, config.currency)} />
                  <Row label="Total Sale" value={formatMinor(summary.totalSalesMinor, config.currency)} bold />
                  <Row label="Expected Cash" value={formatMinor(summary.expectedCashMinor, config.currency)} bold />
                  {summary.declaredCashAmountMinor !== null && (
                    <Row label="Declared Amount" value={formatMinor(summary.declaredCashAmountMinor, config.currency)} />
                  )}
                  {summary.differenceMinor !== null && (
                    <Row
                      label={summary.differenceMinor < 0 ? "Short" : "Over"}
                      value={formatMinor(Math.abs(summary.differenceMinor), config.currency)}
                      bold
                      tone={summary.differenceMinor < 0 ? "text-red-600" : "text-emerald-600"}
                    />
                  )}

                  <div className="flex gap-2 pt-1">
                    <button
                      type="button"
                      onClick={() => void downloadPdf()}
                      className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-white"
                    >
                      <Download className="h-3.5 w-3.5" />
                      PDF
                    </button>
                    <button
                      type="button"
                      onClick={() => void printThermal(shift.id)}
                      className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-white"
                    >
                      <Printer className="h-3.5 w-3.5" />
                      Print
                    </button>
                  </div>
                  {printStatus && <p className="text-xs text-slate-500">{printStatus}</p>}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function Row({ label, value, bold = false, tone }: { label: string; value: string; bold?: boolean; tone?: string }) {
  return (
    <div className={`flex justify-between ${bold ? "font-semibold" : ""} ${tone ?? "text-slate-700"}`}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
