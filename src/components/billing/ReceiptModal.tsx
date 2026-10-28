import { useEffect, useState } from "react";
import { CheckCircle2, Download, Printer, X } from "lucide-react";
import { useLogoDataUrl } from "../../hooks/useLogoDataUrl";
import { printReceiptThermal } from "../../services/billingService";
import { paymentMethodLabel } from "../../utils/checkoutPayments";
import { formatMinor, formatQty } from "../../utils/format";
import type { AppConfig, Sale } from "../../types";
import { ModalOverlay } from "../ui/ModalOverlay";
import { runAction } from "../../utils/runAction";

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
  const logoUrl = useLogoDataUrl(config.logoPath);
  const [printStatus, setPrintStatus] = useState<string | null>(null);
  const [isPrinting, setIsPrinting] = useState(false);
  const [isSavingPdf, setIsSavingPdf] = useState(false);

  const printThermal = async () => {
    await runAction(setIsPrinting, setPrintStatus, async () => {
      await printReceiptThermal(sale.id);
      setPrintStatus("Sent to thermal printer.");
    });
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
    await runAction(setIsSavingPdf, setPrintStatus, async () => {
      const { downloadReceiptPdf } = await import("../../utils/receiptPdf");
      const saved = await downloadReceiptPdf(sale, config, tablesEnabled);
      setPrintStatus(saved ? "Receipt PDF saved." : null);
    });
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-sm rounded-3xl bg-surface shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-line px-5 py-4">
          <span className="flex items-center gap-2 text-base font-semibold text-success-text">
            <CheckCircle2 className="h-5 w-5" />
            Sale complete
          </span>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-fg-muted hover:bg-surface-3 hover:text-fg-2"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="max-h-[60dvh] overflow-y-auto px-5 py-4">
          {logoUrl && <img src={logoUrl} alt="" className="mx-auto mb-2 max-h-24 max-w-[70%] object-contain" />}
          <p className="text-center text-base font-bold text-fg">{config.businessName}</p>
          <p className="text-center text-xs text-fg-muted">
            Sale #{sale.receiptNo} · {sale.createdAt}
            {sale.tillName ? ` · ${sale.tillName}` : ""}
          </p>
          {sale.orderNo && (
            <p className="mt-1 text-center text-xs font-semibold text-accent-text">
              Order / Token #{sale.orderNo}
            </p>
          )}

          <ul className="mt-4 divide-y divide-line border-y border-line text-sm">
            {sale.items.map((line) => (
              <li key={line.itemId} className="flex justify-between py-1.5">
                <span className="text-fg-2">
                  {line.itemName} <span className="text-fg-muted">×{formatQty(line.qty, line.unit)}</span>
                </span>
                <span className="text-fg">{formatMinor(line.lineTotalMinor, config.currency)}</span>
              </li>
            ))}
          </ul>

          <dl className="mt-3 space-y-1 text-sm">
            <div className="flex justify-between text-fg-muted">
              <dt>Subtotal</dt>
              <dd>{formatMinor(sale.subtotalMinor, config.currency)}</dd>
            </div>
            {sale.discountMinor > 0 && (
              <div className="flex justify-between text-fg-muted">
                <dt>Discount</dt>
                <dd>-{formatMinor(sale.discountMinor, config.currency)}</dd>
              </div>
            )}
            {sale.taxMinor > 0 && (
              <div className="flex justify-between text-fg-muted">
                <dt>Tax</dt>
                <dd>{formatMinor(sale.taxMinor, config.currency)}</dd>
              </div>
            )}
            {sale.serviceChargeMinor > 0 && (
              <div className="flex justify-between text-fg-muted">
                <dt>Service charge</dt>
                <dd>{formatMinor(sale.serviceChargeMinor, config.currency)}</dd>
              </div>
            )}
            <div className="flex justify-between border-t border-line pt-1.5 text-base font-semibold text-fg">
              <dt>Total</dt>
              <dd>{formatMinor(sale.totalMinor, config.currency)}</dd>
            </div>
            {(sale.payments.length > 1 || sale.paymentMethod === "credit") && (
              <div className="space-y-1 border-t border-line pt-2 text-xs">
                {sale.payments.map((part) => (
                  <div
                    key={part.method}
                    className={`flex justify-between ${part.method === "credit" ? "font-semibold text-warning-text" : "text-fg-muted"}`}
                  >
                    <dt>{paymentMethodLabel(part.method)}</dt>
                    <dd>{formatMinor(part.amountMinor, config.currency)}</dd>
                  </div>
                ))}
                {sale.dueDate && <p className="text-right text-warning-text">Due by {sale.dueDate}</p>}
              </div>
            )}
            {sale.payments.some((part) => part.method === "cash") && sale.tenderedMinor != null && (
              <div className="space-y-1 border-t border-line pt-2 text-xs">
                <div className="flex justify-between text-fg-muted">
                  <dt>Cash Received</dt>
                  <dd>{formatMinor(sale.tenderedMinor, config.currency)}</dd>
                </div>
                <div className="flex justify-between font-semibold text-success-text">
                  <dt>Change to Return</dt>
                  <dd>
                    {formatMinor(
                      sale.changeMinor ?? Math.max(0, sale.tenderedMinor - sale.totalMinor),
                      config.currency,
                    )}
                  </dd>
                </div>
              </div>
            )}
          </dl>

          <p className="mt-3 text-center text-xs text-fg-muted">
            Paid by {paymentMethodLabel(sale.paymentMethod)}
            {sale.tableName ? ` · ${sale.tableName}` : ""}
            {sale.customerName ? ` · ${sale.customerName}` : ""}
          </p>
        </div>

        <div className="space-y-2 border-t border-line px-5 py-4">
          {printStatus && <p className="text-center text-xs text-fg-muted">{printStatus}</p>}
          <button
            type="button"
            onClick={() => void printThermal()}
            disabled={isPrinting}
            className="flex w-full items-center justify-center gap-1.5 rounded-md bg-accent px-3 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:opacity-50"
          >
            <Printer className="h-4 w-4" />
            {isPrinting ? "Printing…" : "Print (thermal)"}
          </button>
          <button
            type="button"
            onClick={() => void savePdf()}
            disabled={isSavingPdf}
            className="flex w-full items-center justify-center gap-1.5 rounded-md border border-line px-3 py-2 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
          >
            <Download className="h-4 w-4" />
            {isSavingPdf ? "Saving…" : "Save as PDF"}
          </button>
          <button
            type="button"
            onClick={onClose}
            className="w-full rounded-md px-3 py-2 text-sm font-medium text-fg-muted hover:bg-surface-3"
          >
            New sale
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
