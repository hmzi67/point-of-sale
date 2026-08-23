import { useEffect, useState } from "react";
import { Download, Printer, Wallet, X } from "lucide-react";
import { getShiftSummary } from "../../services/shiftsService";
import { useShiftStore } from "../../store";
import { useDebouncedValue } from "../../hooks/useDebouncedValue";
import { formatMinor } from "../../utils/format";
import type { AppConfig, ShiftSummary } from "../../types";

interface CloseShiftModalProps {
  shiftId: number;
  config: AppConfig;
  onClose: () => void;
}

/** Declare-cash-count → live reconciliation preview → confirm → print,
 * matching the reference's "Counter-N Sale Details" receipt fields. The
 * preview calls `shift_get_summary` with the in-progress declared amount
 * (never persisted — see that command's doc comment) so Short/Over is
 * visible before the cashier commits to closing anything. */
export function CloseShiftModal({ shiftId, config, onClose }: CloseShiftModalProps) {
  const [declaredInput, setDeclaredInput] = useState("");
  const [preview, setPreview] = useState<ShiftSummary | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [closed, setClosed] = useState<ShiftSummary | null>(null);
  const [isClosing, setIsClosing] = useState(false);
  const [printStatus, setPrintStatus] = useState<string | null>(null);
  const closeShift = useShiftStore((state) => state.close);

  const declaredMinor = declaredInput.trim() === "" ? null : Math.round((Number(declaredInput) || 0) * 100);
  const debouncedDeclared = useDebouncedValue(declaredMinor, 200);

  useEffect(() => {
    if (closed) return; // already closed — stop live-previewing
    getShiftSummary(shiftId, debouncedDeclared)
      .then(setPreview)
      .catch((e: Error) => setPreviewError(e.message));
  }, [shiftId, debouncedDeclared, closed]);

  const confirmClose = async () => {
    if (declaredMinor === null) return;
    setIsClosing(true);
    setPreviewError(null);
    try {
      const summary = await closeShift(declaredMinor);
      setClosed(summary);
    } catch (e) {
      setPreviewError((e as Error).message);
    } finally {
      setIsClosing(false);
    }
  };

  const printThermal = async () => {
    setPrintStatus(null);
    try {
      const { printShiftSummaryThermal } = await import("../../services/shiftsService");
      await printShiftSummaryThermal(shiftId);
      setPrintStatus("Sent to thermal printer.");
    } catch (e) {
      setPrintStatus((e as Error).message);
    }
  };

  const downloadPdf = async () => {
    if (!closed) return;
    try {
      const { downloadShiftSummaryPdf } = await import("../../utils/shiftSummaryPdf");
      await downloadShiftSummaryPdf(closed, config);
    } catch (e) {
      setPrintStatus(`Could not prepare the PDF: ${(e as Error).message}`);
    }
  };

  const summary = closed ?? preview;
  const money = (minor: number) => formatMinor(minor, config.currency);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-3.5">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-slate-900">
            <Wallet className="h-4 w-4 text-brand-600" />
            {closed ? "Shift Closed" : "Close Shift"}
          </h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="max-h-[70dvh] space-y-3 overflow-y-auto px-5 py-4">
          {!closed && (
            <label className="block">
              <span className="mb-1 block text-xs font-medium text-slate-500">
                Declared cash amount ({config.currency})
              </span>
              <input
                type="number"
                min={0}
                step="0.01"
                autoFocus
                value={declaredInput}
                onChange={(e) => setDeclaredInput(e.target.value)}
                placeholder="0.00"
                className="w-full rounded-2xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-sm focus:border-brand-400 focus:outline-none"
              />
            </label>
          )}

          {summary && (
            <dl className="space-y-1.5 rounded-2xl bg-slate-50 p-3.5 text-sm">
              <Row label="Opening Balance" value={money(summary.openingBalanceMinor)} />
              <Row label="Cash Sale" value={money(summary.cashSalesMinor)} />
              <Row label="Card Sale" value={money(summary.cardSalesMinor)} />
              <Row label="Credit Sale" value={money(summary.creditSalesMinor)} />
              <Row label="Other Sale" value={money(summary.otherSalesMinor)} />
              <Row label="Discount Today" value={money(summary.discountMinor)} />
              <Row label="Refund Today" value={money(summary.refundsMinor)} />
              <div className="border-t border-slate-200 pt-1.5">
                <Row label="Total Sale" value={money(summary.totalSalesMinor)} bold />
              </div>
              <Row label="Expected Cash" value={money(summary.expectedCashMinor)} bold />
              {summary.declaredCashAmountMinor !== null && (
                <Row label="Declared Amount" value={money(summary.declaredCashAmountMinor)} />
              )}
              {summary.differenceMinor !== null && (
                <div className="flex justify-between border-t border-slate-200 pt-1.5 text-base font-bold">
                  <dt className={summary.differenceMinor < 0 ? "text-red-600" : "text-emerald-600"}>
                    {summary.differenceMinor < 0 ? "Short" : "Over"}
                  </dt>
                  <dd className={summary.differenceMinor < 0 ? "text-red-600" : "text-emerald-600"}>
                    {money(Math.abs(summary.differenceMinor))}
                  </dd>
                </div>
              )}
            </dl>
          )}

          {previewError && <p className="text-sm text-red-600">{previewError}</p>}
          {printStatus && <p className="text-center text-xs text-slate-500">{printStatus}</p>}
        </div>

        <div className="space-y-2 p-5 pt-1">
          {!closed ? (
            <div className="flex gap-2">
              <button
                type="button"
                onClick={onClose}
                className="flex-1 rounded-2xl border border-slate-200 py-2.5 text-sm font-semibold text-slate-600 hover:bg-slate-50"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void confirmClose()}
                disabled={declaredMinor === null || isClosing}
                className="flex-1 rounded-2xl bg-brand-600 py-2.5 text-sm font-semibold text-white shadow-soft hover:bg-brand-700 disabled:opacity-50"
              >
                {isClosing ? "Closing…" : "Confirm Close"}
              </button>
            </div>
          ) : (
            <>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => void downloadPdf()}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-2xl border border-slate-200 py-2.5 text-sm font-semibold text-slate-700 hover:bg-slate-50"
                >
                  <Download className="h-4 w-4" />
                  PDF
                </button>
                <button
                  type="button"
                  onClick={() => void printThermal()}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-2xl border border-slate-200 py-2.5 text-sm font-semibold text-slate-700 hover:bg-slate-50"
                >
                  <Printer className="h-4 w-4" />
                  Print
                </button>
              </div>
              <button
                type="button"
                onClick={onClose}
                className="w-full rounded-2xl bg-brand-600 py-2.5 text-sm font-semibold text-white shadow-soft hover:bg-brand-700"
              >
                Done
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function Row({ label, value, bold = false }: { label: string; value: string; bold?: boolean }) {
  return (
    <div className={`flex justify-between ${bold ? "font-bold text-slate-900" : "text-slate-600"}`}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
