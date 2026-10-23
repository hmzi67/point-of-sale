import { Plus, Trash2 } from "lucide-react";
import { useModuleEnabled } from "../../hooks/useModules";
import { useBillingStore } from "../../store";
import { decimalToMinor, formatMinor, minorToDecimal } from "../../utils/format";
import { paymentMethodLabel, SPLIT_METHODS, type SplitMethod } from "../../utils/checkoutPayments";

/** The split-payment rows (and, for any pay-later part, the due date). */
export function SplitPaymentEditor({ currency, payableMinor }: { currency: string; payableMinor: number }) {
  const customersEnabled = useModuleEnabled("customers");
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const rows = useBillingStore((state) => state.splitRows);
  const setRows = useBillingStore((state) => state.setSplitRows);
  const dueDate = useBillingStore((state) => state.dueDate);
  const setDueDate = useBillingStore((state) => state.setDueDate);
  const customer = useBillingStore((state) => state.customer);

  const methods = SPLIT_METHODS.filter((m) => customersEnabled || m !== "credit");
  const allocated = rows.reduce((sum, row) => sum + (Number(row.amount) > 0 ? decimalToMinor(Number(row.amount)) : 0), 0);
  const remaining = payableMinor - allocated;
  const hasCredit = paymentMethod === "credit" || rows.some((row) => row.method === "credit" && Number(row.amount) > 0);

  const update = (index: number, patch: Partial<(typeof rows)[number]>) =>
    setRows(rows.map((row, i) => (i === index ? { ...row, ...patch } : row)));

  return (
    <div className="space-y-2 rounded-xl border border-line bg-surface-2/40 p-2.5 text-xs sm:p-3">
      {paymentMethod === "split" && (
        <>
          {rows.map((row, index) => (
            <div key={index} className="flex items-center gap-1.5">
              <select
                value={row.method}
                onChange={(e) => update(index, { method: e.target.value as SplitMethod })}
                aria-label="Payment method"
                className="rounded-lg border border-line-strong bg-surface py-2 pl-2 pr-8 text-xs"
              >
                {methods.map((m) => (
                  <option key={m} value={m}>
                    {paymentMethodLabel(m)}
                  </option>
                ))}
              </select>
              <input
                type="number"
                min={0}
                step="any"
                value={row.amount}
                onChange={(e) => update(index, { amount: e.target.value })}
                placeholder="Amount"
                aria-label="Amount"
                className="min-w-0 flex-1 rounded-lg border border-line-strong bg-surface px-2 py-2 text-xs tabular-nums"
              />
              {remaining > 0 && (
                <button
                  type="button"
                  onClick={() => {
                    const current = Number(row.amount) > 0 ? decimalToMinor(Number(row.amount)) : 0;
                    update(index, { amount: String(minorToDecimal(current + remaining)) });
                  }}
                  className="rounded-lg border border-line px-2 py-2 text-[11px] font-medium text-fg-2 hover:bg-surface-3"
                  title="Put the rest of the bill on this method"
                >
                  Rest
                </button>
              )}
              <button
                type="button"
                onClick={() => setRows(rows.filter((_, i) => i !== index))}
                aria-label="Remove payment"
                className="rounded-lg p-2 text-fg-muted hover:bg-surface-3 hover:text-danger-text"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
          <div className="flex items-center justify-between">
            <button
              type="button"
              onClick={() => setRows([...rows, { method: rows.length === 0 ? "cash" : "card", amount: "" }])}
              className="flex items-center gap-1 font-medium text-accent-text hover:underline"
            >
              <Plus className="h-3.5 w-3.5" /> Add payment
            </button>
            <span className={`tabular-nums font-semibold ${remaining === 0 ? "text-success-text" : "text-warning-text"}`}>
              {remaining === 0 ? "Fully allocated" : remaining > 0 ? `${formatMinor(remaining, currency)} left` : `${formatMinor(-remaining, currency)} over`}
            </span>
          </div>
        </>
      )}
      {hasCredit && (
        <div className="space-y-1.5 border-t border-line pt-2 first:border-t-0 first:pt-0">
          <label className="flex items-center justify-between gap-2">
            <span className="font-semibold text-fg">Due date</span>
            <input
              type="date"
              value={dueDate}
              onChange={(e) => setDueDate(e.target.value)}
              className="rounded-lg border border-line-strong bg-surface px-2 py-1.5 text-xs"
            />
          </label>
          {!customer && <p className="text-warning-text">Choose a customer (top of the order) to sell on credit.</p>}
        </div>
      )}
    </div>
  );
}
