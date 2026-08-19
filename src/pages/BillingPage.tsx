import { useState } from "react";
import { CartPanel } from "../components/billing/CartPanel";
import { DiscountControl } from "../components/billing/DiscountControl";
import { ItemSearchBar } from "../components/billing/ItemSearchBar";
import { PaymentMethodSelector } from "../components/billing/PaymentMethodSelector";
import { ReceiptModal } from "../components/billing/ReceiptModal";
import { TableSelector } from "../components/billing/TableSelector";
import { useModules } from "../hooks/useModules";
import { createSale } from "../services/billingService";
import { useAppConfig } from "../hooks/useAppConfig";
import { useAuthStore, useBillingStore } from "../store";
import { computeCartTotals } from "../utils/billingTotals";
import { formatMinor } from "../utils/format";
import type { Sale } from "../types";

export function BillingPage() {
  const { config } = useAppConfig();
  const cashierId = useAuthStore((state) => state.user?.id ?? null);
  const { modules } = useModules();
  const tablesEnabled = modules.some((m) => m.key === "tables" && m.enabled);

  const cart = useBillingStore((state) => state.cart);
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const tableId = useBillingStore((state) => state.tableId);
  const clearCart = useBillingStore((state) => state.clearCart);

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [completedSale, setCompletedSale] = useState<Sale | null>(null);

  const cartLines = cartOrder.map((id) => cart[id]);
  const totals = computeCartTotals(cartLines, discountMode, discountValue, config.taxPercent);

  const completeSale = async () => {
    if (cartOrder.length === 0) return;
    setIsSubmitting(true);
    setError(null);
    try {
      const sale = await createSale({
        items: cartOrder.map((id) => ({ itemId: id, qty: cart[id].qty })),
        discountMinor: totals.discountMinor,
        taxMinor: totals.taxMinor,
        paymentMethod,
        cashierId,
        tableId,
      });
      setCompletedSale(sale);
      clearCart();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="flex h-full gap-4">
      {/* Cart column */}
      <div className="flex min-w-0 flex-1 flex-col gap-3">
        <ItemSearchBar currency={config.currency} />

        {notice && (
          <p className="rounded-md bg-emerald-50 px-3 py-2 text-sm text-emerald-700">{notice}</p>
        )}

        <div className="flex flex-1 flex-col overflow-hidden rounded-lg border border-slate-200 bg-white">
          <CartPanel currency={config.currency} />
        </div>
      </div>

      {/* Checkout column */}
      <div className="flex w-80 shrink-0 flex-col gap-4 overflow-y-auto rounded-lg border border-slate-200 bg-white p-4">
        <h2 className="text-sm font-semibold text-slate-900">Checkout</h2>

        <DiscountControl />
        <PaymentMethodSelector />
        {tablesEnabled && (
          <TableSelector
            taxPercent={config.taxPercent}
            onParked={(message) => {
              setNotice(message);
              window.setTimeout(() => setNotice(null), 4000);
            }}
          />
        )}

        <dl className="space-y-1.5 border-t border-slate-200 pt-3 text-sm">
          <div className="flex justify-between text-slate-500">
            <dt>Subtotal</dt>
            <dd>{formatMinor(totals.subtotalMinor, config.currency)}</dd>
          </div>
          {totals.discountMinor > 0 && (
            <div className="flex justify-between text-slate-500">
              <dt>Discount</dt>
              <dd>-{formatMinor(totals.discountMinor, config.currency)}</dd>
            </div>
          )}
          {config.taxPercent > 0 && (
            <div className="flex justify-between text-slate-500">
              <dt>Tax ({config.taxPercent}%)</dt>
              <dd>{formatMinor(totals.taxMinor, config.currency)}</dd>
            </div>
          )}
          <div className="flex justify-between border-t border-slate-200 pt-1.5 text-lg font-bold text-slate-900">
            <dt>Total</dt>
            <dd>{formatMinor(totals.totalMinor, config.currency)}</dd>
          </div>
        </dl>

        {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

        <button
          type="button"
          onClick={() => void completeSale()}
          disabled={cartOrder.length === 0 || isSubmitting}
          className="mt-auto rounded-lg bg-brand-600 py-3.5 text-base font-semibold text-white shadow-sm hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSubmitting ? "Completing…" : `Complete Sale · ${formatMinor(totals.totalMinor, config.currency)}`}
        </button>
      </div>

      {completedSale && (
        <ReceiptModal sale={completedSale} config={config} onClose={() => setCompletedSale(null)} />
      )}
    </div>
  );
}
