import { useEffect } from "react";
import { Printer, ShoppingBag } from "lucide-react";
import { ModalCard } from "../ui/ModalCard";
import { formatMinor } from "../../utils/format";

interface PlaceOrderConfirmModalProps {
  itemCount: number;
  totalMinor: number;
  currency: string;
  paymentLabel: string;
  /** Cash change to hand back, when cash was tendered. */
  changeMinor: number | null;
  onPlace: () => void;
  onPlaceAndPrint: () => void;
  onCancel: () => void;
}

/** Asked before a sale is committed: place it quietly, or place it and print
 * the bill. Enter = place, P = place & print, Escape = back to the cart —
 * captured before the billing hotkeys underneath, like `ReceiptModal`. */
export function PlaceOrderConfirmModal({
  itemCount,
  totalMinor,
  currency,
  paymentLabel,
  changeMinor,
  onPlace,
  onPlaceAndPrint,
  onCancel,
}: PlaceOrderConfirmModalProps) {
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const action =
        event.key === "Escape" ? onCancel : event.key === "Enter" ? onPlace : event.key.toLowerCase() === "p" ? onPlaceAndPrint : null;
      if (!action) return;
      event.preventDefault();
      event.stopPropagation();
      action();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [onPlace, onPlaceAndPrint, onCancel]);

  return (
    <ModalCard title="Place this order?" icon={ShoppingBag} width="sm" onClose={onCancel}>
      <dl className="space-y-2 rounded-xl border border-line bg-surface-2/40 p-3 text-sm tabular-nums">
        <div className="flex justify-between text-fg-muted">
          <dt>Items</dt>
          <dd>{itemCount}</dd>
        </div>
        <div className="flex justify-between text-fg-muted">
          <dt>Payment</dt>
          <dd>{paymentLabel}</dd>
        </div>
        {changeMinor !== null && (
          <div className="flex justify-between text-success-text">
            <dt>Change to return</dt>
            <dd>{formatMinor(changeMinor, currency)}</dd>
          </div>
        )}
        <div className="flex justify-between border-t border-dashed border-line-strong pt-2 text-base font-bold text-fg">
          <dt>Total</dt>
          <dd>{formatMinor(totalMinor, currency)}</dd>
        </div>
      </dl>

      <div className="mt-4 grid gap-2">
        <button
          type="button"
          onClick={onPlace}
          autoFocus
          className="rounded-xl bg-accent px-4 py-3 text-sm font-semibold text-accent-ink hover:bg-accent-hover"
        >
          Place order <span className="ml-1 text-xs opacity-70">Enter</span>
        </button>
        <button
          type="button"
          onClick={onPlaceAndPrint}
          className="flex items-center justify-center gap-2 rounded-xl border border-line bg-surface-2 px-4 py-3 text-sm font-semibold text-fg hover:bg-surface-3"
        >
          <Printer className="h-4 w-4 text-accent-text" />
          Place &amp; print bill <span className="text-xs text-fg-muted">P</span>
        </button>
        <button type="button" onClick={onCancel} className="py-1.5 text-sm text-fg-muted hover:text-fg-2">
          Back to cart
        </button>
      </div>
    </ModalCard>
  );
}
