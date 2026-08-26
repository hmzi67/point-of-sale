import { useMemo, useState } from "react";
import { Banknote, X } from "lucide-react";
import { useBillingStore } from "../../store";
import { decimalToMinor, formatMinor, formatQty } from "../../utils/format";

/**
 * "By Amount" entry for a `soldByAmount` item — a cashier types the rupee
 * amount a customer asked for ("100 rupees worth") instead of a quantity,
 * and this shows the computed quantity live (amount ÷ item.price) before
 * committing it to the cart, so the cashier sees what they're giving, not
 * just a hidden calculation. Reads `amountEntryItem` from the billing
 * store — see that field's doc comment for why it's there rather than
 * local state in whichever component (item card or search bar) opened it.
 */
export function ItemAmountEntryModal() {
  const item = useBillingStore((state) => state.amountEntryItem);
  const cancelAmountEntry = useBillingStore((state) => state.cancelAmountEntry);
  const addItemByAmount = useBillingStore((state) => state.addItemByAmount);

  const [amountText, setAmountText] = useState("");

  const amountMinor = useMemo(() => {
    const amount = Number(amountText);
    return Number.isFinite(amount) && amount > 0 ? decimalToMinor(amount) : 0;
  }, [amountText]);

  const computedQty = item && amountMinor > 0 ? amountMinor / item.priceMinor : 0;

  if (!item) return null;

  const confirm = () => {
    if (amountMinor <= 0) return;
    addItemByAmount(item, amountMinor);
    setAmountText("");
    cancelAmountEntry();
  };

  const close = () => {
    setAmountText("");
    cancelAmountEntry();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-sm overflow-hidden rounded-3xl bg-white shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-slate-100 px-5 py-3.5">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-slate-900">
            <Banknote className="h-4 w-4 text-brand-600" />
            {item.name} — by amount
          </h3>
          <button type="button" onClick={close} className="rounded-full p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="space-y-3 px-5 pt-4">
          <p className="text-xs text-slate-500">
            {formatMinor(item.priceMinor)} per {item.unit ?? "unit"}
          </p>

          <label className="block">
            <span className="mb-1 block text-xs font-medium text-slate-500">Amount (PKR)</span>
            <input
              type="number"
              min="0"
              step="0.01"
              inputMode="decimal"
              value={amountText}
              onChange={(e) => setAmountText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  confirm();
                }
              }}
              autoFocus
              placeholder="e.g. 100"
              className="w-full rounded-xl border border-slate-200 bg-slate-50 px-3.5 py-2.5 text-lg font-semibold focus:border-brand-400 focus:outline-none"
            />
          </label>

          <div className="rounded-xl bg-brand-50 px-3.5 py-2.5 text-sm text-brand-800">
            {amountMinor > 0 ? (
              <>
                ≈ <span className="font-semibold">{formatQty(computedQty, item.unit)}</span>
                {computedQty > item.stockQty && (
                  <span className="mt-1 block text-xs text-red-600">
                    Only {formatQty(item.stockQty, item.unit)} in stock — this will be capped.
                  </span>
                )}
              </>
            ) : (
              <span className="text-brand-400">Type an amount to see the quantity</span>
            )}
          </div>
        </div>

        <div className="flex gap-2 p-5 pt-4">
          <button
            type="button"
            onClick={close}
            className="flex-1 rounded-2xl bg-slate-100 py-3 text-sm font-semibold text-slate-600 hover:bg-slate-200"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={confirm}
            disabled={amountMinor <= 0}
            className="flex-1 rounded-2xl bg-brand-600 py-3 text-sm font-semibold text-white shadow-soft hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
          >
            Add to Cart
          </button>
        </div>
      </div>
    </div>
  );
}
