import { useState } from "react";
import { CheckCircle2, Printer, X } from "lucide-react";
import { printReceiptThermal } from "../../services/billingService";
import { formatMinor } from "../../utils/format";
import type { AppConfig, Sale } from "../../types";

interface ReceiptModalProps {
  sale: Sale;
  config: AppConfig;
  onClose: () => void;
}

/** Shown right after a sale completes. Printing itself already happened
 * automatically the moment the sale went through — see `BillingPage.tsx`'s
 * `printReceiptAutomatically` — so this modal's button is a manual
 * *reprint*, for a second physical copy or after fixing a printer that
 * wasn't reachable the first time, not something the cashier has to click
 * to get the first copy out. Thermal print uses whatever printer this
 * installation has configured (USB auto-detected on desktop, Bluetooth
 * selected in Settings on Android) and fails gracefully — with a message —
 * if none is set up or reachable. */
export function ReceiptModal({ sale, config, onClose }: ReceiptModalProps) {
  const [printStatus, setPrintStatus] = useState<string | null>(null);
  const [isPrinting, setIsPrinting] = useState(false);

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
                  {line.itemName} <span className="text-slate-400">×{line.qty}</span>
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
            {isPrinting ? "Printing…" : "Reprint (thermal)"}
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
