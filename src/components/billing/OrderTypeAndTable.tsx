import { useEffect, useState } from "react";
import { getItems } from "../../services/inventoryService";
import { attachCartToTable, getParkedCart, getTables } from "../../services/tablesService";
import { useBillingStore } from "../../store";
import { computeCartTotals } from "../../utils/billingTotals";
import { DropdownSelect } from "../ui/DropdownSelect";
import type { TableSummary } from "../../types";

interface OrderTypeAndTableProps {
  taxPercent: number;
  onParked: (message: string) => void;
}

/** Same free/occupied/reserved color convention as the floor view's
 * `TableCard` — a small dot next to each table option, restyled for a light
 * background instead of losing the status info in the redesign. */
const STATUS_DOT: Record<TableSummary["status"], string> = {
  free: "bg-emerald-500",
  occupied: "bg-red-500",
  reserved: "bg-amber-500",
};

const STATUS_LABEL: Record<TableSummary["status"], string> = {
  free: "Free",
  occupied: "Occupied",
  reserved: "Reserved",
};

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
        <DropdownSelect
          value={tableId}
          placeholder="Select Table"
          disabled={isLoading}
          onChange={(id) => setTableId(id)}
          options={tables.map((table) => ({
            value: table.id,
            label: table.name,
            meta: (
              <span className="flex items-center gap-1.5 text-xs font-medium text-slate-500">
                <span className={`h-1.5 w-1.5 rounded-full ${STATUS_DOT[table.status]}`} />
                {STATUS_LABEL[table.status]}
                {table.hasParkedOrder ? " · parked" : ""}
              </span>
            ),
          }))}
        />

        <DropdownSelect
          value={orderType}
          placeholder="Order Type"
          onChange={(next) => {
            if (next === "takeaway") setTableId(null);
          }}
          options={[
            { value: "dineIn", label: "Dine In", disabled: tableId === null },
            { value: "takeaway", label: "Takeaway" },
          ]}
        />
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
