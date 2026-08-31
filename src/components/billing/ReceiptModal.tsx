import { useEffect, useState } from "react";
import { CheckCircle2, Download, Printer, X } from "lucide-react";
import { printReceiptThermal } from "../../services/billingService";
import { formatMinor, formatQty } from "../../utils/format";
import type { AppConfig, Sale } from "../../types";

interface ReceiptModalProps {
  sale: Sale;
  config: AppConfig;
  tablesEnabled: boolean;
  onClose: () => void;
}

/** Shown right after a sale completes. Nothing prints automatically — the
 * sale is already committed by the time this modal opens, and printing
 * (thermal or PDF) only happens when the cashier explicitly clicks one of
 * the buttons below, so a till with no printer attached (or one the
 * cashier doesn't want to use for a given sale) never blocks or interrupts
 * checkout. Thermal print uses whatever printer this installation has
 * configured (USB auto-detected on desktop, Bluetooth selected in Settings
 * on Android) and fails gracefully — with a message — if none is set up or
 * reachable; "Save as PDF" is the fallback for when there's no thermal
 * printer at all. Both buttons can be clicked more than once (e.g. a
 * second physical copy), same handler either time. */
export function ReceiptModal({ sale, config, tablesEnabled, onClose }: ReceiptModalProps) {
  const [printStatus, setPrintStatus] = useState<string | null>(null);
  const [isPrinting, setIsPrinting] = useState(false);
  const [isSavingPdf, setIsSavingPdf] = useState(false);

  const printThermal = async () => {
    setIsPrinting(true);
    setPrintStatus(null);
    try {
      await printReceiptThermal(sale.id);
      setPrintStatus("Sent to thermal printer.");
    } catch (e) {
      setPrintStatus((e as Error).message);
    } finally {
      setIsPrinting(false);
    }
  };

  // Plain Enter is this dialog's "do the obvious thing": send the receipt to
  // the thermal printer, same as clicking Print. Escape is the other
  // obvious thing: close the dialog, same as clicking the X / "New sale".
  // Both registered in the capture phase so they run and stop propagation
  // before the billing-surface keyboard listener (`useFastBillingHotkeys`,
  // still mounted underneath) sees the event — this dialog is the only
  // thing focused/active while it's open. Mounted/unmounted with the modal
  // itself, so once it closes these keys go right back to meaning whatever
  // they mean elsewhere (item-code confirm/clear, etc.) with zero residual
  // effect here.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Enter") return;
      event.preventDefault();
      event.stopPropagation();
      if (isPrinting) return;
      void printThermal();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isPrinting, onClose]);

  const savePdf = async () => {
    setIsSavingPdf(true);
    setPrintStatus(null);
    try {
      const { downloadReceiptPdf } = await import("../../utils/receiptPdf");
      const saved = await downloadReceiptPdf(sale, config, tablesEnabled);
      setPrintStatus(saved ? "Receipt PDF saved." : null);
    } catch (e) {
      setPrintStatus((e as Error).message);
    } finally {
      setIsSavingPdf(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
          <span className="flex items-center gap-2 text-base font-semibold text-emerald-700">
            <CheckCircle2 className="h-5 w-5" />
            Sale complete
          </span>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="max-h-[60dvh] overflow-y-auto px-5 py-4">
          <p className="text-center text-sm text-slate-500">{config.businessName}</p>
          <p className="text-center text-xs text-slate-400">
            Sale #{sale.id} · {sale.createdAt}
          </p>

          <ul className="mt-4 divide-y divide-slate-100 border-y border-slate-100 text-sm">
            {sale.items.map((line) => (
              <li key={line.itemId} className="flex justify-between py-1.5">
                <span className="text-slate-700">
                  {line.itemName} <span className="text-slate-400">×{formatQty(line.qty, line.unit)}</span>
                </span>
                <span className="text-slate-900">{formatMinor(line.lineTotalMinor, config.currency)}</span>
              </li>
            ))}
          </ul>

          <dl className="mt-3 space-y-1 text-sm">
            <div className="flex justify-between text-slate-500">
              <dt>Subtotal</dt>
              <dd>{formatMinor(sale.subtotalMinor, config.currency)}</dd>
            </div>
            {sale.discountMinor > 0 && (
              <div className="flex justify-between text-slate-500">
                <dt>Discount</dt>
                <dd>-{formatMinor(sale.discountMinor, config.currency)}</dd>
              </div>
            )}
            {sale.taxMinor > 0 && (
              <div className="flex justify-between text-slate-500">
                <dt>Tax</dt>
                <dd>{formatMinor(sale.taxMinor, config.currency)}</dd>
              </div>
            )}
            <div className="flex justify-between border-t border-slate-200 pt-1.5 text-base font-semibold text-slate-900">
              <dt>Total</dt>
              <dd>{formatMinor(sale.totalMinor, config.currency)}</dd>
            </div>
          </dl>

          <p className="mt-3 text-center text-xs text-slate-500">
            Paid by {sale.paymentMethod}
            {sale.tableName ? ` · ${sale.tableName}` : ""}
          </p>
        </div>

        <div className="space-y-2 border-t border-slate-200 px-5 py-4">
          {printStatus && <p className="text-center text-xs text-slate-500">{printStatus}</p>}
          <button
            type="button"
            onClick={() => void printThermal()}
            disabled={isPrinting}
            className="flex w-full items-center justify-center gap-1.5 rounded-md bg-brand-600 px-3 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-50"
          >
            <Printer className="h-4 w-4" />
            {isPrinting ? "Printing…" : "Print (thermal)"}
          </button>
          <button
            type="button"
            onClick={() => void savePdf()}
            disabled={isSavingPdf}
            className="flex w-full items-center justify-center gap-1.5 rounded-md border border-slate-200 px-3 py-2 text-sm font-medium text-slate-600 hover:bg-slate-50 disabled:opacity-50"
          >
            <Download className="h-4 w-4" />
            {isSavingPdf ? "Saving…" : "Save as PDF"}
          </button>
          <button
            type="button"
            onClick={onClose}
            className="w-full rounded-md px-3 py-2 text-sm font-medium text-slate-500 hover:bg-slate-100"
          >
            New sale
          </button>
        </div>
      </div>
    </div>
  );
}
