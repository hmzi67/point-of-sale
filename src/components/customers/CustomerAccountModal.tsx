import { useEffect, useState } from "react";
import { getCreditSales, getCustomer, getCustomerStatement } from "../../services/customersService";
import type { CreditSale, Customer } from "../../types";
import { formatMinor } from "../../utils/format";
import { StatementView } from "../ledger/StatementView";
import { ModalCard, primaryButtonClass } from "../ui/ModalCard";
import { ReceivePaymentModal } from "./ReceivePaymentModal";

/** One customer's account: open pay-later bills (with what's due when) and
 * their running statement. */
export function CustomerAccountModal({
  customer: initial,
  currency,
  onChanged,
  onClose,
}: {
  customer: Customer;
  currency: string;
  onChanged: () => void;
  onClose: () => void;
}) {
  const [customer, setCustomer] = useState(initial);
  const [tab, setTab] = useState<"bills" | "statement">("bills");
  const [bills, setBills] = useState<CreditSale[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [paying, setPaying] = useState<{ saleId: number | null } | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    Promise.all([getCustomer(initial.id), getCreditSales(initial.id, false)])
      .then(([fresh, list]) => {
        setCustomer(fresh);
        setBills(list);
      })
      .catch((e: Error) => setError(e.message));
  }, [initial.id, reloadKey]);

  return (
    <ModalCard
      title={customer.name}
      subtitle={[customer.phone, customer.email].filter(Boolean).join(" · ") || undefined}
      width="lg"
      onClose={onClose}
    >
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <div className="rounded-xl border border-line bg-surface-2/40 px-4 py-2.5">
          <p className="text-[11px] text-fg-muted">Balance</p>
          <p className="text-lg font-bold tabular-nums text-fg">{formatMinor(customer.balanceMinor, currency)}</p>
        </div>
        {customer.overdueMinor > 0 && (
          <div className="rounded-xl border border-danger-line bg-danger-soft px-4 py-2.5">
            <p className="text-[11px] text-danger-text">Overdue</p>
            <p className="text-lg font-bold tabular-nums text-danger-text">{formatMinor(customer.overdueMinor, currency)}</p>
          </div>
        )}
        <div className="ml-auto flex gap-1 rounded-xl border border-line p-1">
          {(["bills", "statement"] as const).map((t) => (
            <button
              key={t}
              type="button"
              aria-pressed={tab === t}
              onClick={() => setTab(t)}
              className={`rounded-lg px-3 py-1.5 text-xs font-semibold ${tab === t ? "bg-accent text-accent-ink" : "text-fg-muted"}`}
            >
              {t === "bills" ? "Pay-later bills" : "Statement"}
            </button>
          ))}
        </div>
        {customer.balanceMinor > 0 && (
          <button type="button" onClick={() => setPaying({ saleId: null })} className={primaryButtonClass}>
            Receive payment
          </button>
        )}
      </div>
      {error && <p className="mb-3 rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      {tab === "bills" ? (
        <div className="overflow-x-auto rounded-xl border border-line">
          <table className="w-full text-sm">
            <thead className="bg-surface-2 text-left text-xs text-fg-muted">
              <tr>
                <th className="px-3 py-2 font-medium">Sale</th>
                <th className="px-3 py-2 font-medium">Due</th>
                <th className="px-3 py-2 text-right font-medium">On credit</th>
                <th className="px-3 py-2 text-right font-medium">Paid</th>
                <th className="px-3 py-2 text-right font-medium">Outstanding</th>
                <th className="px-3 py-2" />
              </tr>
            </thead>
            <tbody className="divide-y divide-line tabular-nums">
              {bills.map((bill) => (
                <tr key={bill.saleId}>
                  <td className="px-3 py-2">
                    <span className="font-medium text-fg">#{bill.saleId}</span>
                    <span className="block text-xs text-fg-muted">{bill.createdAt.slice(0, 10)}</span>
                  </td>
                  <td className="px-3 py-2">
                    {bill.dueDate ?? "—"}
                    {bill.daysOverdue > 0 && <span className="block text-xs font-semibold text-danger-text">{bill.daysOverdue} days overdue</span>}
                  </td>
                  <td className="px-3 py-2 text-right">{formatMinor(bill.creditMinor, currency)}</td>
                  <td className="px-3 py-2 text-right text-success-text">{formatMinor(bill.paidMinor + bill.refundedMinor, currency)}</td>
                  <td className="px-3 py-2 text-right font-semibold">
                    {bill.outstandingMinor > 0 ? formatMinor(bill.outstandingMinor, currency) : <span className="text-success-text">Settled</span>}
                  </td>
                  <td className="px-3 py-2 text-right">
                    {bill.outstandingMinor > 0 && (
                      <button type="button" onClick={() => setPaying({ saleId: bill.saleId })} className="text-xs font-semibold text-accent-text hover:underline">
                        Receive
                      </button>
                    )}
                  </td>
                </tr>
              ))}
              {bills.length === 0 && (
                <tr>
                  <td colSpan={6} className="px-3 py-6 text-center text-xs text-fg-muted">No pay-later sales for this customer.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      ) : (
        <StatementView
          load={(start, end) => getCustomerStatement(customer.id, start, end)}
          currency={currency}
          debitLabel="Owed"
          creditLabel="Paid"
          partyName={customer.name}
          reloadKey={reloadKey}
        />
      )}

      {paying && (
        <ReceivePaymentModal
          customer={customer}
          saleId={paying.saleId}
          currency={currency}
          onClose={() => setPaying(null)}
          onSaved={() => {
            setPaying(null);
            setReloadKey((k) => k + 1);
            onChanged();
          }}
        />
      )}
    </ModalCard>
  );
}
