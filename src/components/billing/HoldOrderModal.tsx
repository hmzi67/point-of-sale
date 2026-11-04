import { useState, type FormEvent } from "react";
import { Check, ChefHat, Clock, Pause, Ticket } from "lucide-react";
import { createAndHoldOrder, printOrderToken, updateAndHoldOrder } from "../../services/ordersService";
import { reportWarning } from "../../services/logService";
import { useBillingStore } from "../../store";
import type { Order } from "../../types";
import { formatMinor } from "../../utils/format";
import { ModalHeader } from "../ui/ModalCard";
import { ModalOverlay } from "../ui/ModalOverlay";
import { DEFAULT_PREP_MINUTES, ORDER_TYPE_LABELS } from "../../utils/orderType";
import { runAction } from "../../utils/runAction";

interface HoldOrderModalProps {
  currency: string;
  totalMinor: number;
  discountMinor: number;
  onClose: () => void;
  onOrderHeld: (order: Order, printError?: string) => void;
}

const PRESET_TIMES = [10, 15, 20, 25, 30, 45];

/** What can print when the order is held — both off by default. */
const PRINTS = [
  { isKitchen: true, label: "Kitchen receipt", icon: ChefHat },
  { isKitchen: false, label: "Customer token", icon: Ticket },
] as const;

export function HoldOrderModal({
  currency,
  totalMinor,
  discountMinor,
  onClose,
  onOrderHeld,
}: HoldOrderModalProps) {
  const cart = useBillingStore((state) => state.cart);
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const tableId = useBillingStore((state) => state.tableId);
  const clearCart = useBillingStore((state) => state.clearCart);
  const activeOrderId = useBillingStore((state) => state.activeOrderId);
  const activeOrderVersion = useBillingStore((state) => state.activeOrderVersion);
  const activeOrderNo = useBillingStore((state) => state.activeOrderNo);
  const activeOrderHold = useBillingStore((state) => state.activeOrderHold);
  const draftCustomerName = useBillingStore((state) => state.customerName);
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);

  const [customerName, setCustomerName] = useState(draftCustomerName);
  const [notes, setNotes] = useState(activeOrderHold?.notes ?? "");
  const [prepMinutes, setPrepMinutes] = useState(activeOrderHold?.prepMinutes ?? DEFAULT_PREP_MINUTES);
  const [printKitchen, setPrintKitchen] = useState(false);
  const [printCustomer, setPrintCustomer] = useState(false);
  const willPrint = printKitchen || printCustomer;
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const currentOrderType = useBillingStore((state) => state.orderType);
  const deliveryDetails = useBillingStore((state) => state.deliveryDetails);
  const orderType = tableId !== null ? "dine_in" : currentOrderType;

  const handleHold = async (e: FormEvent) => {
    e.preventDefault();
    if (cartOrder.length === 0) return;

    await runAction(setIsSubmitting, setError, async () => {
      const lines = cartOrder.map((id) => ({
        itemId: id,
        qty: cart[id].qty,
        notes: cart[id].notes || null,
      }));

      const details = {
        orderType,
        tableId,
        customerName: customerName.trim() || null,
        notes: notes.trim() || null,
        estimatedPrepMinutes: prepMinutes > 0 ? prepMinutes : DEFAULT_PREP_MINUTES,
        delivery: deliveryDetails,
        lines,
      };
      const discount = {
        minor: discountMinor,
        percent: discountMode === "percent" && discountMinor > 0 ? discountValue : null,
      };
      // A resumed cart is that order, edited — never a second order.
      const order =
        activeOrderId !== null && activeOrderVersion !== null
          ? await updateAndHoldOrder(activeOrderId, activeOrderVersion, details, discount)
          : await createAndHoldOrder({ clientRef: crypto.randomUUID(), ...details }, discount);

      // Only what the cashier ticked. The order is already held, so a print
      // failure is reported, never thrown.
      const printErrors: string[] = [];
      for (const { isKitchen, label } of PRINTS) {
        if (!(isKitchen ? printKitchen : printCustomer)) continue;
        try {
          await printOrderToken(order.id, isKitchen);
        } catch (printErr) {
          const message = (printErr as Error).message;
          printErrors.push(`${label} didn't print (${message})`);
          reportWarning(`hold order: ${label} print failed for order ${order.id}: ${message}`);
        }
      }

      clearCart();
      onOrderHeld(order, printErrors.join("; ") || undefined);
      onClose();
    });
  };

  return (
    <ModalOverlay>
      <div className="w-full max-w-md rounded-2xl border border-line bg-surface p-6 shadow-soft-lg">
        <ModalHeader
          icon={Pause}
          title={activeOrderId !== null ? `Update Order #${activeOrderNo ?? activeOrderId}` : "Hold Active Order"}
          subtitle={activeOrderId !== null ? "Save your changes to this order & clear cart" : "Save to queue & clear cart for the next customer"}
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

          {/* Optional prints */}
          <div>
            <span className="mb-1.5 block text-xs font-medium text-fg">Print (Optional)</span>
            <div className="grid grid-cols-2 gap-2">
              {PRINTS.map(({ isKitchen, label, icon: Icon }) => {
                const on = isKitchen ? printKitchen : printCustomer;
                return (
                  <button
                    key={label}
                    type="button"
                    aria-pressed={on}
                    onClick={() => (isKitchen ? setPrintKitchen : setPrintCustomer)(!on)}
                    className={`flex items-center gap-2 rounded-xl border px-3 py-2.5 text-sm font-medium transition ${
                      on ? "border-accent/50 bg-accent-soft text-accent-text" : "border-line bg-surface-2 text-fg-muted hover:bg-surface-3"
                    }`}
                  >
                    <Icon className="h-4 w-4 shrink-0" />
                    <span className="flex-1 text-left">{label}</span>
                    <span
                      className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                        on ? "border-accent bg-accent text-accent-ink" : "border-line-strong"
                      }`}
                    >
                      {on && <Check className="h-3 w-3" />}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Cart preview summary */}
          <div className="flex items-center justify-between rounded-xl border border-line bg-surface-2/60 p-3 text-xs text-fg-muted">
            <span>
              {cartOrder.length} {cartOrder.length === 1 ? "item" : "items"} (
                {ORDER_TYPE_LABELS[orderType]}
              )
            </span>
            <span className="text-right font-semibold text-fg">
              {discountMinor > 0 && (
                <span className="block font-medium text-fg-muted">
                  Discount: -{formatMinor(discountMinor, currency)}
                </span>
              )}
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
              {isSubmitting ? "Holding..." : willPrint ? "Hold & Print" : "Hold & Clear Cart"}
            </button>
          </div>
        </form>
      </div>
    </ModalOverlay>
  );
}
