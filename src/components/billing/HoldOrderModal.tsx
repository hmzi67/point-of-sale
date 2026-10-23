import { useState, type FormEvent } from "react";
import { Clock, Pause } from "lucide-react";
import { createAndHoldOrder, printOrderToken } from "../../services/ordersService";
import { reportWarning } from "../../services/logService";
import { useBillingStore } from "../../store";
import type { Order } from "../../types";
import { formatMinor } from "../../utils/format";
import { ModalHeader } from "../ui/ModalCard";
import { ModalOverlay } from "../ui/ModalOverlay";
import { ORDER_TYPE_LABELS } from "../../utils/orderType";
import { runAction } from "../../utils/runAction";

interface HoldOrderModalProps {
  currency: string;
  totalMinor: number;
  onClose: () => void;
  onOrderHeld: (order: Order, tokenError?: string) => void;
}

const PRESET_TIMES = [10, 15, 20, 25, 30, 45];

export function HoldOrderModal({
  currency,
  totalMinor,
  onClose,
  onOrderHeld,
}: HoldOrderModalProps) {
  const cart = useBillingStore((state) => state.cart);
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const tableId = useBillingStore((state) => state.tableId);
  const clearCart = useBillingStore((state) => state.clearCart);

  const [customerName, setCustomerName] = useState("");
  const [notes, setNotes] = useState("");
  const [prepMinutes, setPrepMinutes] = useState(15);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const currentOrderType = useBillingStore((state) => state.orderType);
  const deliveryDetails = useBillingStore((state) => state.deliveryDetails);
  const orderType = tableId !== null ? "dine_in" : currentOrderType;

  const handleHold = async (e: FormEvent) => {
    e.preventDefault();
    if (cartOrder.length === 0) return;

    await runAction(setIsSubmitting, setError, async () => {
      const clientRef = crypto.randomUUID();
      const lines = cartOrder.map((id) => ({
        itemId: id,
        qty: cart[id].qty,
        notes: cart[id].notes || null,
      }));

      const order = await createAndHoldOrder({
        clientRef,
        orderType,
        tableId,
        customerName: customerName.trim() || null,
        notes: notes.trim() || null,
        estimatedPrepMinutes: prepMinutes > 0 ? prepMinutes : 15,
        delivery: deliveryDetails,
        lines,
      });

      // Automatically trigger token printing (kitchen ticket)
      let tokenError: string | undefined;
      try {
        await printOrderToken(order.id, true);
      } catch (printErr) {
        tokenError = (printErr as Error).message;
        reportWarning(`hold order: token print failed for order ${order.id}: ${tokenError}`);
      }

      clearCart();
      onOrderHeld(order, tokenError);
      onClose();
    });
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-md rounded-2xl border border-line bg-surface p-6 shadow-soft-lg">
        <ModalHeader
          icon={Pause}
          title="Hold Active Order"
          subtitle="Save to queue & clear cart for the next customer"
          onClose={onClose}
          className="pb-4"
        />

        <form onSubmit={handleHold} className="mt-5 space-y-4">
          {error && (
            <div className="rounded-xl bg-danger-soft p-3 text-xs text-danger-text">
              {error}
            </div>
          )}

          {/* Customer / Order Identifier */}
          <div>
            <label className="mb-1.5 block text-xs font-medium text-fg">
              Customer Name / Token Label (Optional)
            </label>
            <input
              type="text"
              autoFocus
              placeholder="e.g. John, TK-1, Blue Shirt..."
              value={customerName}
              onChange={(e) => setCustomerName(e.target.value)}
              className="w-full rounded-xl border border-line bg-surface-2 px-3.5 py-2.5 text-sm text-fg outline-none transition focus:border-accent"
            />
          </div>

          {/* Custom Preparation Time */}
          <div>
            <div className="mb-1.5 flex items-center justify-between">
              <label className="flex items-center gap-1.5 text-xs font-medium text-fg">
                <Clock className="h-3.5 w-3.5 text-accent-text" />
                Custom Preparation Target (Minutes)
              </label>
              <span className="text-xs font-semibold text-accent-text">
                {prepMinutes} min
              </span>
            </div>

            {/* Quick chips */}
            <div className="grid grid-cols-6 gap-1.5">
              {PRESET_TIMES.map((minutes) => (
                <button
                  key={minutes}
                  type="button"
                  onClick={() => setPrepMinutes(minutes)}
                  className={`rounded-lg py-1.5 text-xs font-medium transition ${
                    prepMinutes === minutes
                      ? "bg-accent text-accent-ink shadow-sm"
                      : "border border-line bg-surface-2 text-fg-muted hover:bg-surface-3"
                  }`}
                >
                  {minutes}m
                </button>
              ))}
            </div>

            {/* Stepper for custom minutes */}
            <div className="mt-2 flex items-center gap-2">
              <button
                type="button"
                onClick={() => setPrepMinutes((m) => Math.max(1, m - 5))}
                className="rounded-lg border border-line bg-surface-2 px-3 py-1.5 text-xs font-semibold text-fg hover:bg-surface-3"
              >
                - 5 min
              </button>
              <input
                type="number"
                min={1}
                max={180}
                value={prepMinutes}
                onChange={(e) => setPrepMinutes(Math.max(1, parseInt(e.target.value) || 1))}
                className="w-full rounded-lg border border-line bg-surface-2 py-1.5 text-center text-xs font-semibold text-fg outline-none focus:border-accent"
              />
              <button
                type="button"
                onClick={() => setPrepMinutes((m) => m + 5)}
                className="rounded-lg border border-line bg-surface-2 px-3 py-1.5 text-xs font-semibold text-fg hover:bg-surface-3"
              >
                + 5 min
              </button>
            </div>
          </div>

          {/* Optional notes */}
          <div>
            <label className="mb-1.5 block text-xs font-medium text-fg">
              Kitchen / Order Note (Optional)
            </label>
            <input
              type="text"
              placeholder="e.g. less spicy, pack separately..."
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
              className="w-full rounded-xl border border-line bg-surface-2 px-3.5 py-2 text-sm text-fg outline-none transition focus:border-accent"
            />
          </div>

          {/* Cart preview summary */}
          <div className="flex items-center justify-between rounded-xl border border-line bg-surface-2/60 p-3 text-xs text-fg-muted">
            <span>
              {cartOrder.length} {cartOrder.length === 1 ? "item" : "items"} (
                {ORDER_TYPE_LABELS[orderType]}
              )
            </span>
            <span className="font-semibold text-fg">
              Total: {formatMinor(totalMinor, currency)}
            </span>
          </div>

          {/* Action buttons */}
          <div className="flex items-center justify-end gap-2.5 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={isSubmitting}
              className="rounded-xl border border-line px-4 py-2.5 text-sm font-medium text-fg-muted hover:bg-surface-2"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isSubmitting || cartOrder.length === 0}
              className="rounded-xl bg-accent px-5 py-2.5 text-sm font-semibold text-accent-ink shadow-soft hover:opacity-95 disabled:opacity-50"
            >
              {isSubmitting ? "Holding..." : "Hold & Clear Cart"}
            </button>
          </div>
        </form>
      </div>
    </ModalOverlay>
  );
}
