import { useEffect, useState } from "react";
import { Download, Printer, Wallet, X } from "lucide-react";
import { getShiftSummary } from "../../services/shiftsService";
import { useShiftStore } from "../../store";
import { useDebouncedValue } from "../../hooks/useDebouncedValue";
import { useShiftSummaryOutput } from "../../hooks/useShiftSummaryOutput";
import { decimalToMinor, formatMinor } from "../../utils/format";
import type { AppConfig, ShiftSummary } from "../../types";
import { ModalOverlay } from "../ui/ModalOverlay";
import { runAction } from "../../utils/runAction";

interface CloseShiftModalProps {
  shiftId: number;
  config: AppConfig;
  onClose: () => void;
}

/** Declare-cash-count → live reconciliation preview → confirm → print,
 * matching the reference's "Counter-N Sale Details" receipt fields. The
 * preview calls `shifts_get_summary` with the in-progress declared amount
 * (never persisted — see that command's doc comment) so Short/Over is
 * visible before the cashier commits to closing anything. */
export function CloseShiftModal({ shiftId, config, onClose }: CloseShiftModalProps) {
  const [declaredInput, setDeclaredInput] = useState("");
  const [preview, setPreview] = useState<ShiftSummary | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [closed, setClosed] = useState<ShiftSummary | null>(null);
  const [isClosing, setIsClosing] = useState(false);
  const closeShift = useShiftStore((state) => state.close);
  const { status: printStatus, printThermal, downloadPdf } = useShiftSummaryOutput(config);

  const declaredMinor = declaredInput.trim() === "" ? null : decimalToMinor(Number(declaredInput) || 0);
  const debouncedDeclared = useDebouncedValue(declaredMinor, 200);

  useEffect(() => {
    if (closed) return; // already closed — stop live-previewing
    getShiftSummary(shiftId, debouncedDeclared)
      .then(setPreview)
      .catch((e: Error) => setPreviewError(e.message));
  }, [shiftId, debouncedDeclared, closed]);

  const confirmClose = async () => {
    if (declaredMinor === null) return;
    await runAction(setIsClosing, setPreviewError, async () => {
      const summary = await closeShift(declaredMinor);
      setClosed(summary);
    });
  };

  const summary = closed ?? preview;
  const money = (minor: number) => formatMinor(minor, config.currency);

  return (
    <ModalOverlay>
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-surface shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-fg">
            <Wallet className="h-4 w-4 text-accent-text" />
            {closed ? "Shift Closed" : "Close Shift"}
          </h3>
          <button type="button" onClick={onClose} className="rounded-full p-1 text-fg-muted hover:bg-surface-3 hover:text-fg-2">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="max-h-[70dvh] space-y-3 overflow-y-auto px-5 py-4">
          {!closed && (
            <label className="block">
              <span className="mb-1 block text-xs font-medium text-fg-muted">
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
                className="w-full rounded-2xl border border-line bg-surface-2 px-3.5 py-2.5 text-sm focus:border-accent focus:outline-none"
              />
            </label>
          )}

          {summary && (
            <dl className="space-y-1.5 rounded-2xl bg-surface-2 p-3.5 text-sm">
              <Row label="Opening Balance" value={money(summary.openingBalanceMinor)} />
              <Row label="Cash Sale" value={money(summary.cashSalesMinor)} />
              <Row label="Card Sale" value={money(summary.cardSalesMinor)} />
              <Row label="Credit Sale" value={money(summary.creditSalesMinor)} />
              <Row label="Other Sale" value={money(summary.otherSalesMinor)} />
              <Row label="Discount Today" value={money(summary.discountMinor)} />
              <Row label="Refund Today" value={money(summary.refundsMinor)} />
              <div className="border-t border-line pt-1.5">
                <Row label="Total Sale" value={money(summary.totalSalesMinor)} bold />
              </div>
              <Row label="Expected Cash" value={money(summary.expectedCashMinor)} bold />
              {summary.declaredCashAmountMinor !== null && (
                <Row label="Declared Amount" value={money(summary.declaredCashAmountMinor)} />
              )}
              {summary.differenceMinor !== null && (
                <div className="flex justify-between border-t border-line pt-1.5 text-base font-bold">
                  <dt className={summary.differenceMinor < 0 ? "text-danger-text" : "text-success-text"}>
                    {summary.differenceMinor < 0 ? "Short" : "Over"}
                  </dt>
                  <dd className={summary.differenceMinor < 0 ? "text-danger-text" : "text-success-text"}>
                    {money(Math.abs(summary.differenceMinor))}
                  </dd>
                </div>
              )}
            </dl>
          )}

          {previewError && <p className="text-sm text-danger-text">{previewError}</p>}
          {printStatus && <p className="text-center text-xs text-fg-muted">{printStatus}</p>}
        </div>

        <div className="space-y-2 p-5 pt-1">
          {!closed ? (
            <div className="flex gap-2">
              <button
                type="button"
                onClick={onClose}
                className="flex-1 rounded-2xl border border-line py-2.5 text-sm font-semibold text-fg-2 hover:bg-surface-2"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void confirmClose()}
                disabled={declaredMinor === null || isClosing}
                className="flex-1 rounded-2xl bg-accent py-2.5 text-sm font-semibold text-accent-ink shadow-soft hover:bg-accent-hover disabled:opacity-50"
              >
                {isClosing ? "Closing…" : "Confirm Close"}
              </button>
            </div>
          ) : (
            <>
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => closed && void downloadPdf(closed)}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-2xl border border-line py-2.5 text-sm font-semibold text-fg-2 hover:bg-surface-2"
                >
                  <Download className="h-4 w-4" />
                  PDF
                </button>
                <button
                  type="button"
                  onClick={() => void printThermal(shiftId)}
                  className="flex flex-1 items-center justify-center gap-1.5 rounded-2xl border border-line py-2.5 text-sm font-semibold text-fg-2 hover:bg-surface-2"
                >
                  <Printer className="h-4 w-4" />
                  Print
                </button>
              </div>
              <button
                type="button"
                onClick={onClose}
                className="w-full rounded-2xl bg-accent py-2.5 text-sm font-semibold text-accent-ink shadow-soft hover:bg-accent-hover"
              >
                Done
              </button>
            </>
          )}
        </div>
      </div>
    </ModalOverlay>
  );
}

function Row({ label, value, bold = false }: { label: string; value: string; bold?: boolean }) {
  return (
    <div className={`flex justify-between ${bold ? "font-bold text-fg" : "text-fg-2"}`}>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
