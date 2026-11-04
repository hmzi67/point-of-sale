import { useEffect, useRef, useState } from "react";
import { CheckCircle2, Download, Printer, X } from "lucide-react";
import { useLogoDataUrl } from "../../hooks/useLogoDataUrl";
import { printReceiptThermal } from "../../services/billingService";
import { ReceiptPaper } from "./ReceiptPaper";
import type { AppConfig, Sale } from "../../types";
import { ModalOverlay } from "../ui/ModalOverlay";
import { runAction } from "../../utils/runAction";

interface ReceiptModalProps {
  sale: Sale;
  config: AppConfig;
  tablesEnabled: boolean;
  /** Send to the thermal printer once on open ("Place & print bill"). */
  autoPrint?: boolean;
  onClose: () => void;
}

/** Shown after a sale placed with "Place & print bill", which sends it to
 * the thermal printer once on open (`autoPrint`). The sale is already
 * committed by then, so a print failure only shows a message here — it
 * never blocks or undoes checkout. Thermal print uses whatever printer this installation has
 * configured (USB auto-detected on desktop, Bluetooth selected in Settings
 * on Android) and fails gracefully — with a message — if none is set up or
 * reachable; "Save as PDF" is the fallback for when there's no thermal
 * printer at all. Both buttons can be clicked more than once (e.g. a
 * second physical copy), same handler either time. */
export function ReceiptModal({ sale, config, tablesEnabled, autoPrint = false, onClose }: ReceiptModalProps) {
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

  // A ref, because StrictMode runs this effect twice in dev, which sent two
  // print jobs at once and left the printer stuck part-way through the logo.
  const autoPrinted = useRef(false);
  useEffect(() => {
    if (!autoPrint || autoPrinted.current) return;
    autoPrinted.current = true;
    void printThermal();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

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

        <div className="max-h-[60dvh] overflow-y-auto bg-surface-2 px-4 py-4">
          <ReceiptPaper sale={sale} config={config} tablesEnabled={tablesEnabled} logoUrl={logoUrl} />
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
