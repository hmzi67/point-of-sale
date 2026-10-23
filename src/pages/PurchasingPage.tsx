import { useState } from "react";
import { ClipboardList, PackageCheck, Plus, Truck, Wallet } from "lucide-react";
import { StatementView } from "../components/ledger/StatementView";
import { PurchaseOrderModal } from "../components/purchasing/PurchaseOrderModal";
import { ReceiveGoodsModal } from "../components/purchasing/ReceiveGoodsModal";
import { SupplierFormModal } from "../components/purchasing/SupplierFormModal";
import { SupplierPaymentModal } from "../components/purchasing/SupplierPaymentModal";
import { DataTable, EmptyRow } from "../components/ui/DataTable";
import { ManagementSummary } from "../components/ui/ManagementSummary";
import { ModalCard, primaryButtonClass, secondaryButtonClass } from "../components/ui/ModalCard";
import { useAppConfig } from "../hooks/useAppConfig";
import { useReport } from "../hooks/useReport";
import { getItems } from "../services/inventoryService";
import {
  getIncomingStock,
  getSupplierStatement,
  listGoodsReceipts,
  listPurchaseOrders,
  listSupplierPayments,
  listSuppliers,
  setPurchaseOrderStatus,
  setSupplierActive,
} from "../services/purchasingService";
import type { GoodsReceipt, IncomingLine, Item, PoStatus, PurchaseOrder, Supplier, SupplierPayment } from "../types";
import { paymentMethodLabel } from "../utils/checkoutPayments";
import { formatMinor, formatQty } from "../utils/format";

type Tab = "orders" | "receipts" | "incoming" | "suppliers" | "payments";

const TABS: { key: Tab; label: string }[] = [
  { key: "orders", label: "Purchase orders" },
  { key: "receipts", label: "Goods received" },
  { key: "incoming", label: "Incoming stock" },
  { key: "suppliers", label: "Suppliers" },
  { key: "payments", label: "Supplier payments" },
];

const STATUS_STYLE: Record<PoStatus, string> = {
  draft: "bg-surface-3 text-fg-muted",
  ordered: "bg-accent-soft text-accent-text",
  partial: "bg-warning-soft text-warning-text",
  received: "bg-success-soft text-success-text",
  cancelled: "bg-danger-soft text-danger-text",
};

/** Suppliers, purchase orders, goods received notes and what's owed. */
export function PurchasingPage() {
  const { config } = useAppConfig();
  const currency = config.currency;
  const [tab, setTab] = useState<Tab>("orders");
  const [statusFilter, setStatusFilter] = useState<PoStatus | "">("");
  const [showArchived, setShowArchived] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const [editingOrder, setEditingOrder] = useState<PurchaseOrder | null | undefined>(undefined);
  const [receiving, setReceiving] = useState<PurchaseOrder | null | undefined>(undefined);
  const [editingSupplier, setEditingSupplier] = useState<Supplier | null | undefined>(undefined);
  const [paying, setPaying] = useState<Supplier | null>(null);
  const [statementFor, setStatementFor] = useState<Supplier | null>(null);
  const [viewingReceipt, setViewingReceipt] = useState<GoodsReceipt | null>(null);

  const loaded = useReport(
    () =>
      Promise.all([
        listSuppliers(showArchived),
        getItems({}),
        listPurchaseOrders(statusFilter || null),
        listGoodsReceipts(null),
        getIncomingStock(),
        listSupplierPayments(null),
      ]),
    [statusFilter, showArchived],
  );
  const [suppliers, items, orders, receipts, incoming, payments]: [
    Supplier[],
    Item[],
    PurchaseOrder[],
    GoodsReceipt[],
    IncomingLine[],
    SupplierPayment[],
  ] = loaded.data ?? [[], [], [], [], [], []];
  const { isLoading, reload } = loaded;
  const error = actionError ?? loaded.error;

  const run = async (action: () => Promise<unknown>) => {
    try {
      setActionError(null);
      await action();
      reload();
    } catch (e) {
      setActionError((e as Error).message);
    }
  };

  const activeSuppliers = suppliers.filter((s) => s.isActive);
  const payable = suppliers.reduce((sum, s) => sum + Math.max(0, s.balanceMinor), 0);
  const openOrders = orders.filter((o) => o.status === "ordered" || o.status === "partial");
  const needsSupplier = activeSuppliers.length === 0;

  return (
    <section className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Supply chain</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Purchasing</h2>
          <p className="mt-2 max-w-2xl text-sm text-fg-muted">Order from suppliers, receive goods into stock (GRN) and track what you owe.</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <button type="button" disabled={needsSupplier} onClick={() => setReceiving(null)} className={`${secondaryButtonClass} flex items-center gap-1.5`}>
            <PackageCheck className="h-4 w-4" /> Receive goods
          </button>
          <button type="button" disabled={needsSupplier} onClick={() => setEditingOrder(null)} className={`${primaryButtonClass} flex items-center gap-1.5`}>
            <Plus className="h-4 w-4" /> New order
          </button>
        </div>
      </div>

      <ManagementSummary
        isLoading={isLoading}
        items={[
          { label: "You owe suppliers", value: formatMinor(payable, currency), detail: "Goods received minus payments", icon: Wallet },
          { label: "Open orders", value: openOrders.length, detail: `${incoming.length} line(s) still to arrive`, icon: Truck, tone: "warning" },
          { label: "Suppliers", value: activeSuppliers.length, detail: "Active supplier accounts", icon: ClipboardList, tone: "success" },
        ]}
      />

      {needsSupplier && !isLoading && (
        <p className="rounded-2xl border border-warning-line bg-warning-soft px-4 py-3 text-sm text-warning-text">
          Add a supplier first — purchase orders and goods receipts are always against one.
        </p>
      )}

      <div className="flex flex-wrap gap-2 rounded-2xl border border-line bg-surface p-2">
        {TABS.map((t) => (
          <button
            key={t.key}
            type="button"
            aria-pressed={tab === t.key}
            onClick={() => setTab(t.key)}
            className={`rounded-xl px-4 py-2 text-xs font-semibold ${tab === t.key ? "bg-accent-soft text-accent-text" : "text-fg-muted hover:bg-surface-2"}`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      {tab === "orders" && (
        <div className="space-y-3">
          <select value={statusFilter} onChange={(e) => setStatusFilter(e.target.value as PoStatus | "")} className="rounded-xl border border-line-strong py-2 pl-3 pr-8 text-sm">
            <option value="">All statuses</option>
            {(["draft", "ordered", "partial", "received", "cancelled"] as const).map((s) => (
              <option key={s} value={s}>{s}</option>
            ))}
          </select>
          <DataTable head={["PO", "Supplier", "Dates", "Items", "Total", "Status", ""]}>
            {orders.map((order) => (
              <tr key={order.id}>
                <td className="px-4 py-3 font-semibold text-fg">#{order.id}</td>
                <td className="px-4 py-3 text-fg-2">{order.supplierName}</td>
                <td className="px-4 py-3 text-xs text-fg-muted">
                  Ordered {order.orderDate}
                  {order.expectedDate && <span className="block">Expected {order.expectedDate}</span>}
                </td>
                <td className="px-4 py-3 text-xs text-fg-2">
                  {order.lines.map((l) => (
                    <span key={l.id} className="block">
                      {l.itemName} — {formatQty(l.qtyReceived, l.unit)} / {formatQty(l.qtyOrdered, l.unit)}
                    </span>
                  ))}
                </td>
                <td className="px-4 py-3 text-right tabular-nums">{formatMinor(order.totalMinor, currency)}</td>
                <td className="px-4 py-3">
                  <span className={`rounded-lg px-2 py-1 text-[11px] font-semibold capitalize ${STATUS_STYLE[order.status]}`}>{order.status}</span>
                </td>
                <td className="whitespace-nowrap px-4 py-3 text-right text-xs font-semibold">
                  {order.status === "draft" && (
                    <>
                      <button type="button" onClick={() => setEditingOrder(order)} className="mr-3 text-fg-2 hover:underline">Edit</button>
                      <button type="button" onClick={() => void run(() => setPurchaseOrderStatus(order.id, "ordered"))} className="mr-3 text-accent-text hover:underline">
                        Place
                      </button>
                    </>
                  )}
                  {(order.status === "ordered" || order.status === "partial") && (
                    <button type="button" onClick={() => setReceiving(order)} className="mr-3 text-accent-text hover:underline">Receive</button>
                  )}
                  {(order.status === "draft" || order.status === "ordered" || order.status === "partial") && (
                    <button type="button" onClick={() => void run(() => setPurchaseOrderStatus(order.id, "cancelled"))} className="text-fg-muted hover:underline">
                      Cancel
                    </button>
                  )}
                </td>
              </tr>
            ))}
            {orders.length === 0 && <EmptyRow span={7} text="No purchase orders yet." />}
          </DataTable>
        </div>
      )}

      {tab === "receipts" && (
        <DataTable head={["GRN", "Supplier", "Received", "Invoice", "Items", "Total"]}>
          {receipts.map((grn) => (
            <tr key={grn.id} className="cursor-pointer hover:bg-surface-2" onClick={() => setViewingReceipt(grn)}>
              <td className="px-4 py-3 font-semibold text-fg">#{grn.id}{grn.poId && <span className="block text-xs font-normal text-fg-muted">PO #{grn.poId}</span>}</td>
              <td className="px-4 py-3 text-fg-2">{grn.supplierName}</td>
              <td className="px-4 py-3 text-xs text-fg-muted">{grn.receivedDate}{grn.receivedByName && <span className="block">by {grn.receivedByName}</span>}</td>
              <td className="px-4 py-3 text-fg-2">{grn.invoiceRef ?? "—"}</td>
              <td className="px-4 py-3 text-xs text-fg-2">{grn.lines.length} line(s)</td>
              <td className="px-4 py-3 text-right tabular-nums">{formatMinor(grn.totalCostMinor, currency)}</td>
            </tr>
          ))}
          {receipts.length === 0 && <EmptyRow span={6} text="Nothing received yet." />}
        </DataTable>
      )}

      {tab === "incoming" && (
        <DataTable head={["Item", "Supplier", "PO", "Still to come", "Value", "Expected"]}>
          {incoming.map((line) => (
            <tr key={line.poItemId}>
              <td className="px-4 py-3 font-medium text-fg">{line.itemName}</td>
              <td className="px-4 py-3 text-fg-2">{line.supplierName}</td>
              <td className="px-4 py-3 text-fg-2">#{line.poId}</td>
              <td className="px-4 py-3 tabular-nums">{formatQty(line.qtyOutstanding, line.unit)}</td>
              <td className="px-4 py-3 text-right tabular-nums">{formatMinor(Math.round(line.qtyOutstanding * line.unitCostMinor), currency)}</td>
              <td className="px-4 py-3 text-xs text-fg-muted">{line.expectedDate ?? "—"}</td>
            </tr>
          ))}
          {incoming.length === 0 && <EmptyRow span={6} text="Nothing on order." />}
        </DataTable>
      )}

      {tab === "suppliers" && (
        <div className="space-y-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <label className="flex items-center gap-2 text-sm text-fg-2">
              <input type="checkbox" checked={showArchived} onChange={(e) => setShowArchived(e.target.checked)} /> Show archived
            </label>
            <button type="button" onClick={() => setEditingSupplier(null)} className={`${primaryButtonClass} flex items-center gap-1.5`}>
              <Plus className="h-4 w-4" /> Add supplier
            </button>
          </div>
          <DataTable head={["Supplier", "Contact", "Purchased", "Paid", "Balance", ""]}>
            {suppliers.map((s) => (
              <tr key={s.id} className={s.isActive ? "" : "opacity-60"}>
                <td className="px-4 py-3 font-semibold text-fg">{s.name}{s.notes && <span className="block text-xs font-normal text-fg-muted">{s.notes}</span>}</td>
                <td className="px-4 py-3 text-xs text-fg-2">
                  {[s.contactPerson, s.phone, s.email].filter(Boolean).join(" · ") || "—"}
                </td>
                <td className="px-4 py-3 text-right tabular-nums">{formatMinor(s.purchasedMinor, currency)}</td>
                <td className="px-4 py-3 text-right tabular-nums text-success-text">{formatMinor(s.paidMinor, currency)}</td>
                <td className="px-4 py-3 text-right font-semibold tabular-nums">{formatMinor(s.balanceMinor, currency)}</td>
                <td className="whitespace-nowrap px-4 py-3 text-right text-xs font-semibold">
                  <button type="button" onClick={() => setPaying(s)} className="mr-3 text-accent-text hover:underline">Pay</button>
                  <button type="button" onClick={() => setStatementFor(s)} className="mr-3 text-fg-2 hover:underline">Ledger</button>
                  <button type="button" onClick={() => setEditingSupplier(s)} className="mr-3 text-fg-2 hover:underline">Edit</button>
                  <button type="button" onClick={() => void run(() => setSupplierActive(s.id, !s.isActive))} className="text-fg-muted hover:underline">
                    {s.isActive ? "Archive" : "Restore"}
                  </button>
                </td>
              </tr>
            ))}
            {suppliers.length === 0 && <EmptyRow span={6} text="No suppliers yet." />}
          </DataTable>
        </div>
      )}

      {tab === "payments" && (
        <DataTable head={["Date", "Supplier", "Method", "Reference", "Amount"]}>
          {payments.map((p) => (
            <tr key={p.id}>
              <td className="px-4 py-3 text-fg-muted">{p.paidDate}</td>
              <td className="px-4 py-3 font-medium text-fg">{p.supplierName}</td>
              <td className="px-4 py-3 text-fg-2">{paymentMethodLabel(p.method)}</td>
              <td className="px-4 py-3 text-fg-2">{p.reference ?? p.note ?? "—"}</td>
              <td className="px-4 py-3 text-right tabular-nums">{formatMinor(p.amountMinor, currency)}</td>
            </tr>
          ))}
          {payments.length === 0 && <EmptyRow span={5} text="No supplier payments recorded." />}
        </DataTable>
      )}

      {editingOrder !== undefined && (
        <PurchaseOrderModal
          order={editingOrder}
          suppliers={activeSuppliers}
          items={items}
          currency={currency}
          onClose={() => setEditingOrder(undefined)}
          onSaved={() => {
            setEditingOrder(undefined);
            reload();
          }}
        />
      )}
      {receiving !== undefined && (
        <ReceiveGoodsModal
          order={receiving}
          suppliers={activeSuppliers}
          items={items}
          currency={currency}
          onClose={() => setReceiving(undefined)}
          onSaved={() => {
            setReceiving(undefined);
            reload();
          }}
        />
      )}
      {editingSupplier !== undefined && (
        <SupplierFormModal
          supplier={editingSupplier}
          onClose={() => setEditingSupplier(undefined)}
          onSaved={() => {
            setEditingSupplier(undefined);
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
      {statementFor && (
        <ModalCard title={`${statementFor.name} — ledger`} subtitle="Goods received vs. payments made" width="lg" onClose={() => setStatementFor(null)}>
          <StatementView
            load={(start, end) => getSupplierStatement(statementFor.id, start, end)}
            currency={currency}
            debitLabel="Purchased"
            creditLabel="Paid"
            partyName={statementFor.name}
          />
        </ModalCard>
      )}
      {viewingReceipt && (
        <ModalCard
          title={`GRN #${viewingReceipt.id} — ${viewingReceipt.supplierName}`}
          subtitle={`Received ${viewingReceipt.receivedDate}${viewingReceipt.invoiceRef ? ` · invoice ${viewingReceipt.invoiceRef}` : ""}`}
          width="lg"
          onClose={() => setViewingReceipt(null)}
        >
          <DataTable head={["Item", "Qty", "Unit cost", "Batch", "Expiry", "Total"]}>
            {viewingReceipt.lines.map((l, i) => (
              <tr key={i}>
                <td className="px-4 py-2 text-fg">{l.itemName}</td>
                <td className="px-4 py-2 tabular-nums">{formatQty(l.qty, l.unit)}</td>
                <td className="px-4 py-2 text-right tabular-nums">{formatMinor(l.unitCostMinor, currency)}</td>
                <td className="px-4 py-2 text-fg-2">{l.batchNo ?? "—"}</td>
                <td className="px-4 py-2 text-fg-2">{l.expiryDate ?? "—"}</td>
                <td className="px-4 py-2 text-right tabular-nums">{formatMinor(l.lineTotalMinor, currency)}</td>
              </tr>
            ))}
          </DataTable>
          {viewingReceipt.notes && <p className="mt-3 text-sm text-fg-muted">{viewingReceipt.notes}</p>}
        </ModalCard>
      )}
    </section>
  );
}
