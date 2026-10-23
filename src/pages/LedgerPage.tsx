import { useEffect, useState } from "react";
import { AlertTriangle, ArrowDownLeft, ArrowUpRight, Scale } from "lucide-react";
import { ReceivePaymentModal } from "../components/customers/ReceivePaymentModal";
import { StatementView } from "../components/ledger/StatementView";
import { SupplierPaymentModal } from "../components/purchasing/SupplierPaymentModal";
import { DataTable, EmptyRow } from "../components/ui/DataTable";
import { ModalCard } from "../components/ui/ModalCard";
import { useAppConfig } from "../hooks/useAppConfig";
import { useModuleEnabled } from "../hooks/useModules";
import { getCustomer, getCustomerStatement } from "../services/customersService";
import { getLedgerSummary, getSupplierStatement, listSuppliers } from "../services/purchasingService";
import type { Customer, LedgerParty, LedgerSummary, Supplier } from "../types";
import { formatMinor } from "../utils/format";

type StatementTarget = { kind: "customer" | "supplier"; party: LedgerParty };

/** Receivables and payables at a glance — who owes the shop, whom the shop
 * owes — with each party's running statement. */
export function LedgerPage() {
  const { config } = useAppConfig();
  const currency = config.currency;
  const customersEnabled = useModuleEnabled("customers");
  const [summary, setSummary] = useState<LedgerSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [statement, setStatement] = useState<StatementTarget | null>(null);
  const [receiving, setReceiving] = useState<Customer | null>(null);
  const [paying, setPaying] = useState<Supplier | null>(null);

  const reload = () => {
    getLedgerSummary()
      .then((result) => {
        setSummary(result);
        setError(null);
      })
      .catch((e: Error) => setError(e.message));
  };
  useEffect(reload, []);

  const openReceive = (party: LedgerParty) =>
    getCustomer(party.id)
      .then(setReceiving)
      .catch((e: Error) => setError(e.message));
  const openPay = (party: LedgerParty) =>
    listSuppliers(true)
      .then((list) => setPaying(list.find((s) => s.id === party.id) ?? null))
      .catch((e: Error) => setError(e.message));

  const cards = summary
    ? [
        { label: "Receivable", value: summary.receivableMinor, detail: `${summary.debtors.length} customer(s) owe you`, icon: ArrowDownLeft, tone: "text-accent-text bg-accent-soft" },
        { label: "Overdue", value: summary.overdueMinor, detail: `${summary.overdue.saleCount} bill(s) late · ${summary.overdue.dueSoonCount} due in 3 days`, icon: AlertTriangle, tone: "text-danger-text bg-danger-soft" },
        { label: "Payable", value: summary.payableMinor, detail: `${summary.creditors.length} supplier(s) to pay`, icon: ArrowUpRight, tone: "text-warning-text bg-warning-soft" },
        { label: "Net position", value: summary.receivableMinor - summary.payableMinor, detail: "Receivable minus payable", icon: Scale, tone: "text-success-text bg-success-soft" },
      ]
    : [];

  return (
    <section className="space-y-6">
      <div>
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Accounts</p>
        <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Ledger</h2>
        <p className="mt-2 max-w-2xl text-sm text-fg-muted">What customers owe you and what you owe suppliers, with full statements.</p>
      </div>

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {cards.map(({ label, value, detail, icon: Icon, tone }) => (
          <div key={label} className="rounded-2xl border border-line bg-surface p-5">
            <div className="flex items-center justify-between">
              <p className="text-xs font-medium text-fg-muted">{label}</p>
              <span className={`flex h-9 w-9 items-center justify-center rounded-xl ${tone}`}><Icon className="h-4 w-4" /></span>
            </div>
            <p className="mt-3 text-2xl font-bold tabular-nums text-fg">{formatMinor(value, currency)}</p>
            <p className="mt-2 text-[11px] text-fg-muted">{detail}</p>
          </div>
        ))}
      </div>

      {summary && (
        <p className="text-xs text-fg-muted">
          This month: collected {formatMinor(summary.collectedThisMonthMinor, currency)} from customers · paid{" "}
          {formatMinor(summary.paidOutThisMonthMinor, currency)} to suppliers.
        </p>
      )}

      <div className="grid grid-cols-1 gap-5 xl:grid-cols-2">
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-fg">Customers who owe you</h3>
          <DataTable head={["Customer", "Balance", "Overdue", ""]}>
            {summary?.debtors.map((party) => (
              <tr key={party.id}>
                <td className="px-4 py-3 font-medium text-fg">{party.name}<span className="block text-xs font-normal text-fg-muted">{party.phone ?? ""}</span></td>
                <td className="px-4 py-3 text-right tabular-nums">{formatMinor(party.balanceMinor, currency)}</td>
                <td className={`px-4 py-3 text-right tabular-nums ${party.overdueMinor > 0 ? "font-semibold text-danger-text" : "text-fg-muted"}`}>
                  {party.overdueMinor > 0 ? formatMinor(party.overdueMinor, currency) : "—"}
                </td>
                <td className="whitespace-nowrap px-4 py-3 text-right text-xs font-semibold">
                  {customersEnabled && (
                    <button type="button" onClick={() => void openReceive(party)} className="mr-3 text-accent-text hover:underline">Receive</button>
                  )}
                  <button type="button" onClick={() => setStatement({ kind: "customer", party })} className="text-fg-2 hover:underline">Statement</button>
                </td>
              </tr>
            ))}
            {summary && summary.debtors.length === 0 && <EmptyRow span={4} text="No customer owes anything." />}
          </DataTable>
        </div>
        <div className="space-y-2">
          <h3 className="text-sm font-semibold text-fg">Suppliers you owe</h3>
          <DataTable head={["Supplier", "Balance", ""]}>
            {summary?.creditors.map((party) => (
              <tr key={party.id}>
                <td className="px-4 py-3 font-medium text-fg">{party.name}<span className="block text-xs font-normal text-fg-muted">{party.phone ?? ""}</span></td>
                <td className="px-4 py-3 text-right tabular-nums">{formatMinor(party.balanceMinor, currency)}</td>
                <td className="whitespace-nowrap px-4 py-3 text-right text-xs font-semibold">
                  <button type="button" onClick={() => void openPay(party)} className="mr-3 text-accent-text hover:underline">Pay</button>
                  <button type="button" onClick={() => setStatement({ kind: "supplier", party })} className="text-fg-2 hover:underline">Statement</button>
                </td>
              </tr>
            ))}
            {summary && summary.creditors.length === 0 && <EmptyRow span={3} text="Nothing owed to suppliers." />}
          </DataTable>
        </div>
      </div>

      {statement && (
        <ModalCard title={`${statement.party.name} — statement`} width="lg" onClose={() => setStatement(null)}>
          <StatementView
            load={(start, end) =>
              statement.kind === "customer"
                ? getCustomerStatement(statement.party.id, start, end)
                : getSupplierStatement(statement.party.id, start, end)
            }
            currency={currency}
            debitLabel={statement.kind === "customer" ? "Owed" : "Purchased"}
            creditLabel="Paid"
            partyName={statement.party.name}
          />
        </ModalCard>
      )}
      {receiving && (
        <ReceivePaymentModal
          customer={receiving}
          currency={currency}
          onClose={() => setReceiving(null)}
          onSaved={() => {
            setReceiving(null);
            reload();
          }}
        />
      )}
      {paying && (
        <SupplierPaymentModal
          supplier={paying}
          currency={currency}
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
