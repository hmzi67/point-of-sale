import { useState } from "react";
import { AlertTriangle, Plus, Search, UsersRound, Wallet } from "lucide-react";
import { CustomerAccountModal } from "../components/customers/CustomerAccountModal";
import { CustomerFormModal } from "../components/customers/CustomerFormModal";
import { ReceivePaymentModal } from "../components/customers/ReceivePaymentModal";
import { ManagementSummary, PersonAvatar } from "../components/ui/ManagementSummary";
import { primaryButtonClass } from "../components/ui/ModalCard";
import { useAppConfig } from "../hooks/useAppConfig";
import { useDebouncedValue } from "../hooks/useDebouncedValue";
import { useReport } from "../hooks/useReport";
import { getOverdueSales, listCustomers, setCustomerActive } from "../services/customersService";
import { useAuthStore } from "../store";
import type { CreditSale, Customer } from "../types";
import { formatMinor } from "../utils/format";
import { isAdminRole } from "../utils/permissions";

/** Customer accounts: contact details, what each owes on pay-later sales,
 * overdue alerts, installments and statements. */
export function CustomersPage() {
  const { config } = useAppConfig();
  const role = useAuthStore((state) => state.user?.role ?? "cashier");
  const [search, setSearch] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [onlyOwing, setOnlyOwing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [editing, setEditing] = useState<Customer | null | undefined>(undefined);
  const [viewing, setViewing] = useState<Customer | null>(null);
  const [paying, setPaying] = useState<Customer | null>(null);

  const debouncedSearch = useDebouncedValue(search, 150);
  const accounts = useReport(
    () => Promise.all([listCustomers(debouncedSearch.trim() || null, showArchived), getOverdueSales()]),
    [debouncedSearch, showArchived],
  );
  const [customers, overdue]: [Customer[], CreditSale[]] = accounts.data ?? [[], []];
  const { isLoading, reload } = accounts;
  const error = actionError ?? accounts.error;

  const visible = onlyOwing ? customers.filter((c) => c.balanceMinor > 0) : customers;
  const receivable = customers.reduce((sum, c) => sum + c.balanceMinor, 0);
  const overdueTotal = overdue.reduce((sum, s) => sum + s.outstandingMinor, 0);

  const toggleArchived = async (customer: Customer) => {
    try {
      setActionError(null);
      await setCustomerActive(customer.id, !customer.isActive);
      reload();
    } catch (e) {
      setActionError((e as Error).message);
    }
  };

  return (
    <section className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Accounts</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Customers</h2>
          <p className="mt-2 max-w-2xl text-sm text-fg-muted">Customer details, pay-later balances, installments and payment alerts.</p>
        </div>
        <button type="button" onClick={() => setEditing(null)} className={`${primaryButtonClass} flex items-center gap-1.5`}>
          <Plus className="h-4 w-4" /> Add customer
        </button>
      </div>

      <ManagementSummary
        isLoading={isLoading}
        items={[
          { label: "Owed to you", value: formatMinor(receivable, config.currency), detail: "Outstanding pay-later balances", icon: Wallet },
          { label: "Overdue", value: formatMinor(overdueTotal, config.currency), detail: `${overdue.length} bill(s) past their due date`, icon: AlertTriangle, tone: "warning" },
          { label: "Customers", value: customers.length, detail: showArchived ? "Including archived" : "Active accounts", icon: UsersRound, tone: "success" },
        ]}
      />

      {overdue.length > 0 && (
        <div className="rounded-2xl border border-danger-line bg-danger-soft p-4">
          <p className="flex items-center gap-2 text-sm font-semibold text-danger-text">
            <AlertTriangle className="h-4 w-4" /> Payment alerts
          </p>
          <ul className="mt-2 space-y-1 text-sm">
            {overdue.slice(0, 6).map((sale) => (
              <li key={sale.saleId} className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-fg">
                  <button
                    type="button"
                    className="font-semibold hover:underline"
                    onClick={() => {
                      const match = customers.find((c) => c.id === sale.customerId);
                      if (match) setViewing(match);
                    }}
                  >
                    {sale.customerName}
                  </button>
                  <span className="text-fg-muted"> · sale #{sale.saleId} · due {sale.dueDate}</span>
                </span>
                <span className="tabular-nums font-semibold text-danger-text">
                  {formatMinor(sale.outstandingMinor, config.currency)} · {sale.daysOverdue}d late
                </span>
              </li>
            ))}
            {overdue.length > 6 && <li className="text-xs text-danger-text">and {overdue.length - 6} more…</li>}
          </ul>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3 rounded-2xl border border-line bg-surface p-4">
        <div className="relative min-w-56 flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search name, phone or email"
            className="w-full rounded-xl border border-line-strong py-2.5 pl-9 pr-3 text-sm"
          />
        </div>
        <label className="flex items-center gap-2 text-sm text-fg-2">
          <input type="checkbox" checked={onlyOwing} onChange={(e) => setOnlyOwing(e.target.checked)} /> Owing only
        </label>
        <label className="flex items-center gap-2 text-sm text-fg-2">
          <input type="checkbox" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} /> Show archived
        </label>
      </div>

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
        <table className="w-full text-sm">
          <thead className="border-b border-line text-left text-xs text-fg-muted">
            <tr>
              <th className="px-4 py-3 font-medium">Customer</th>
              <th className="px-4 py-3 font-medium">Contact</th>
              <th className="px-4 py-3 text-right font-medium">Balance</th>
              <th className="px-4 py-3 text-right font-medium">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {visible.map((customer) => (
              <tr key={customer.id} className={customer.isActive ? "" : "opacity-60"}>
                <td className="px-4 py-3">
                  <button type="button" onClick={() => setViewing(customer)} className="flex items-center gap-3 text-left">
                    <PersonAvatar name={customer.name} />
                    <span>
                      <span className="block font-semibold text-fg hover:underline">{customer.name}</span>
                      {customer.address && <span className="block text-xs text-fg-muted">{customer.address}</span>}
                    </span>
                  </button>
                </td>
                <td className="px-4 py-3 text-fg-2">
                  <span className="block">{customer.phone ?? "—"}</span>
                  {customer.email && <span className="block text-xs text-fg-muted">{customer.email}</span>}
                </td>
                <td className="px-4 py-3 text-right tabular-nums">
                  {customer.balanceMinor > 0 ? (
                    <>
                      <span className="font-semibold text-fg">{formatMinor(customer.balanceMinor, config.currency)}</span>
                      {customer.overdueMinor > 0 && (
                        <span className="block text-xs font-semibold text-danger-text">{formatMinor(customer.overdueMinor, config.currency)} overdue</span>
                      )}
                    </>
                  ) : (
                    <span className="text-fg-muted">—</span>
                  )}
                </td>
                <td className="whitespace-nowrap px-4 py-3 text-right text-xs font-semibold">
                  {customer.balanceMinor > 0 && (
                    <button type="button" onClick={() => setPaying(customer)} className="mr-3 text-accent-text hover:underline">
                      Receive
                    </button>
                  )}
                  <button type="button" onClick={() => setViewing(customer)} className="mr-3 text-fg-2 hover:underline">
                    Account
                  </button>
                  <button type="button" onClick={() => setEditing(customer)} className="mr-3 text-fg-2 hover:underline">
                    Edit
                  </button>
                  {isAdminRole(role) && (
                    <button type="button" onClick={() => void toggleArchived(customer)} className="text-fg-muted hover:underline">
                      {customer.isActive ? "Archive" : "Restore"}
                    </button>
                  )}
                </td>
              </tr>
            ))}
            {!isLoading && visible.length === 0 && (
              <tr>
                <td colSpan={4} className="px-4 py-10 text-center text-sm text-fg-muted">No customers found.</td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {editing !== undefined && (
        <CustomerFormModal
          customer={editing}
          onClose={() => setEditing(undefined)}
          onSaved={() => {
            setEditing(undefined);
            reload();
          }}
        />
      )}
      {viewing && <CustomerAccountModal customer={viewing} currency={config.currency} onChanged={reload} onClose={() => setViewing(null)} />}
      {paying && (
        <ReceivePaymentModal
          customer={paying}
          currency={config.currency}
          onClose={() => setPaying(null)}
          onSaved={() => {
            setPaying(null);
            reload();
          }}
        />
      )}
    </section>
  );
}
