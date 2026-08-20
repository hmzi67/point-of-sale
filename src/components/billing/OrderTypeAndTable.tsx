import { useEffect, useState } from "react";
import { ChevronDown } from "lucide-react";
import { getItems } from "../../services/inventoryService";
import { attachCartToTable, getParkedCart, getTables } from "../../services/tablesService";
import { useBillingStore } from "../../store";
import { computeCartTotals } from "../../utils/billingTotals";
import type { TableSummary } from "../../types";

interface OrderTypeAndTableProps {
  taxPercent: number;
  onParked: (message: string) => void;
}

const selectClass =
  "w-full appearance-none rounded-2xl border-0 bg-slate-50 py-2.5 pl-4 pr-9 text-sm font-medium text-slate-700 focus:outline-none focus:ring-2 focus:ring-brand-200";

/**
 * The cart panel's Table + Order Type row. All the actual park/resume logic
 * is unchanged from the previous `TableSelector` — only the layout changed,
 * plus a purely-derived "Order Type" reading: `tableId` set means Dine In,
 * `null` means Takeaway. There's no `order_type` column anywhere to store —
 * picking "Takeaway" just clears `tableId`, same as "Counter sale" did
 * before, so this stays wired to the exact same state the backend already
 * understands.
 */
export function OrderTypeAndTable({ taxPercent, onParked }: OrderTypeAndTableProps) {
  const [tables, setTables] = useState<TableSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isBusy, setIsBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const tableId = useBillingStore((state) => state.tableId);
  const setTableId = useBillingStore((state) => state.setTableId);
  const cart = useBillingStore((state) => state.cart);
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);
  const loadParkedCart = useBillingStore((state) => state.loadParkedCart);

  const reloadTables = () => {
    setIsLoading(true);
    getTables()
      .then(setTables)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  };

  useEffect(reloadTables, []);

  const selectedTable = tables.find((t) => t.id === tableId);
  const orderType = tableId !== null ? "dineIn" : "takeaway";

  const saveToTable = async () => {
    if (tableId === null || cartOrder.length === 0) return;
    setIsBusy(true);
    setError(null);
    try {
      const lines = cartOrder.map((id) => ({ itemId: id, qty: cart[id].qty }));
      const { discountMinor } = computeCartTotals(
        cartOrder.map((id) => cart[id]),
        discountMode,
        discountValue,
        taxPercent,
      );
      await attachCartToTable(tableId, lines, discountMinor);
      const tableName = selectedTable?.name ?? "the table";
      useBillingStore.getState().clearCart();
      reloadTables();
      onParked(`Cart saved to ${tableName}.`);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsBusy(false);
    }
  };

  const resumeParkedOrder = async () => {
    if (tableId === null) return;
    setIsBusy(true);
    setError(null);
    try {
      const [parked, items] = await Promise.all([getParkedCart(tableId), getItems({})]);
      if (!parked) return;
      const byId = new Map(items.map((item) => [item.id, item]));
      loadParkedCart(parked.items, parked.discountMinor, (id) => byId.get(id));
      setTableId(tableId);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsBusy(false);
    }
  };

  return (
    <div className="space-y-2">
      <div className="grid grid-cols-2 gap-2.5">
        <div className="relative">
          <select
            value={tableId ?? ""}
            onChange={(e) => setTableId(e.target.value ? Number(e.target.value) : null)}
            disabled={isLoading}
            className={selectClass}
          >
            <option value="">Select Table</option>
            {tables.map((table) => (
              <option key={table.id} value={table.id}>
                {table.name} · {table.status}
                {table.hasParkedOrder ? " · parked" : ""}
              </option>
            ))}
          </select>
          <ChevronDown className="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
        </div>

        <div className="relative">
          <select
            value={orderType}
            onChange={(e) => {
              if (e.target.value === "takeaway") setTableId(null);
            }}
            className={selectClass}
          >
            <option value="dineIn" disabled={tableId === null}>
              Dine In
            </option>
            <option value="takeaway">Takeaway</option>
          </select>
          <ChevronDown className="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
        </div>
      </div>

      {error && <p className="text-xs text-red-600">{error}</p>}

      {selectedTable?.hasParkedOrder && (
        <button
          type="button"
          onClick={() => void resumeParkedOrder()}
          disabled={isBusy}
          className="w-full rounded-xl bg-amber-50 px-3 py-1.5 text-xs font-medium text-amber-700 hover:bg-amber-100 disabled:opacity-50"
        >
          Load parked order from {selectedTable.name}
        </button>
      )}

      {tableId !== null && cartOrder.length > 0 && (
        <button
          type="button"
          onClick={() => void saveToTable()}
          disabled={isBusy}
          className="w-full rounded-xl border border-slate-200 px-3 py-1.5 text-xs font-medium text-slate-600 hover:bg-slate-50 disabled:opacity-50"
        >
          Save to table (bill later)
        </button>
      )}
    </div>
  );
}
