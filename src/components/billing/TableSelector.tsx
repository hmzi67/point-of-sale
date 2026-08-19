import { useEffect, useState } from "react";
import { Utensils } from "lucide-react";
import { getItems } from "../../services/inventoryService";
import { attachCartToTable, getParkedCart, getTables } from "../../services/tablesService";
import { useBillingStore } from "../../store";
import { computeCartTotals } from "../../utils/billingTotals";
import type { TableSummary } from "../../types";

interface TableSelectorProps {
  taxPercent: number;
  onParked: (message: string) => void;
}

/**
 * Only ever rendered when the `tables` module is enabled — see BillingPage,
 * which decides that once, so nothing in here needs its own module check.
 * Lets a cashier tag the current sale with a dine-in table, or park the cart
 * on a table ("Save to table") to bill later instead of completing now.
 */
export function TableSelector({ taxPercent, onParked }: TableSelectorProps) {
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
    <div className="rounded-md border border-slate-200 p-3">
      <span className="flex items-center gap-1.5 text-xs font-medium text-slate-500">
        <Utensils className="h-3.5 w-3.5" />
        Table (optional)
      </span>

      <select
        value={tableId ?? ""}
        onChange={(e) => setTableId(e.target.value ? Number(e.target.value) : null)}
        disabled={isLoading}
        className="mt-1.5 w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm"
      >
        <option value="">Counter sale (no table)</option>
        {tables.map((table) => (
          <option key={table.id} value={table.id}>
            {table.name} · {table.status}
            {table.hasParkedOrder ? " · parked order" : ""}
          </option>
        ))}
      </select>

      {error && <p className="mt-1.5 text-xs text-red-600">{error}</p>}

      {selectedTable?.hasParkedOrder && (
        <button
          type="button"
          onClick={() => void resumeParkedOrder()}
          disabled={isBusy}
          className="mt-2 w-full rounded-md bg-amber-50 px-3 py-1.5 text-xs font-medium text-amber-700 hover:bg-amber-100 disabled:opacity-50"
        >
          Load parked order from {selectedTable.name}
        </button>
      )}

      {tableId !== null && cartOrder.length > 0 && (
        <button
          type="button"
          onClick={() => void saveToTable()}
          disabled={isBusy}
          className="mt-2 w-full rounded-md border border-slate-300 px-3 py-1.5 text-xs font-medium text-slate-700 hover:bg-slate-50 disabled:opacity-50"
        >
          Save to table (bill later)
        </button>
      )}
    </div>
  );
}
