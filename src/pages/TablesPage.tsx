import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Plus } from "lucide-react";
import { AddTableForm } from "../components/tables/AddTableForm";
import { EditTableModal } from "../components/tables/EditTableModal";
import { ShiftTableModal } from "../components/tables/ShiftTableModal";
import { TableCard } from "../components/tables/TableCard";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { getItems } from "../services/inventoryService";
import {
  addTable,
  assignOrderToTable,
  clearTable,
  deleteTable,
  getParkedCart,
  getTables,
  renameTable,
  shiftTableOrder,
  updateTableStatus,
} from "../services/tablesService";
import { useBillingStore } from "../store";
import type { TableSummary } from "../types";

/** Restaurant floor view: one card per table, color-coded by status. Tapping
 * a free/reserved table seats it and jumps into billing pre-linked to that
 * table (reusing BillingPage as-is via `useBillingStore.tableId`, not a
 * duplicate checkout screen); tapping an occupied one resumes its parked
 * cart. A completed sale frees the table automatically server-side
 * (`tables::close_table_order_for_sale`) — this screen just reflects that
 * the next time it's loaded. */
export function TablesPage() {
  const [tables, setTables] = useState<TableSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [shiftingTable, setShiftingTable] = useState<TableSummary | null>(null);
  const [editingTable, setEditingTable] = useState<TableSummary | null>(null);
  const [deletingTable, setDeletingTable] = useState<TableSummary | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const navigate = useNavigate();

  const billingTableId = useBillingStore((state) => state.tableId);
  const setTableId = useBillingStore((state) => state.setTableId);
  const loadParkedCart = useBillingStore((state) => state.loadParkedCart);
  const clearCart = useBillingStore((state) => state.clearCart);

  const reload = () => {
    setIsLoading(true);
    getTables()
      .then(setTables)
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  };

  useEffect(reload, []);

  const withBusy = async (table: TableSummary, action: () => Promise<void>) => {
    setBusyId(table.id);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusyId(null);
    }
  };

  /** Free (or reserved) table tapped: start a fresh empty order, then open
   * billing pre-linked to it. Any leftover counter-sale cart is dropped —
   * it belongs to no table and shouldn't bleed into this one. */
  const seatTable = (table: TableSummary) =>
    withBusy(table, async () => {
      await assignOrderToTable(table.id);
      clearCart();
      setTableId(table.id);
      navigate("/billing");
    });

  /** Occupied table tapped: pull its parked cart back into the billing
   * store, resolving each line against current item data, then open billing. */
  const resumeTable = (table: TableSummary) =>
    withBusy(table, async () => {
      const [parked, items] = await Promise.all([getParkedCart(table.id), getItems({})]);
      if (parked) {
        const byId = new Map(items.map((item) => [item.id, item]));
        loadParkedCart(parked.items, parked.discountMinor, (id) => byId.get(id));
      }
      setTableId(table.id);
      navigate("/billing");
    });

  const releaseTable = (table: TableSummary) =>
    withBusy(table, async () => {
      await clearTable(table.id);
      reload();
    });

  const reserveTable = (table: TableSummary) =>
    withBusy(table, async () => {
      await updateTableStatus(table.id, "reserved");
      reload();
    });

  const handleAddTable = async (name: string, seats: number) => {
    await addTable(name, seats);
    setShowAddForm(false);
    reload();
  };

  const handleRenameTable = async (name: string) => {
    if (!editingTable) return;
    await renameTable(editingTable.id, name);
    setEditingTable(null);
    reload();
  };

  const confirmDeleteTable = async () => {
    if (!deletingTable) return;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      await deleteTable(deletingTable.id);
      setDeletingTable(null);
      reload();
    } catch (e) {
      setDeleteError((e as Error).message);
    } finally {
      setIsDeleting(false);
    }
  };

  /** Confirmed from the Shift Table picker: moves `shiftingTable`'s order
   * onto `toTableId`. If the Billing screen currently has `shiftingTable`
   * selected (the cashier had it open, or just navigated away and back),
   * repoint it at the new table too — otherwise Billing would keep pointing
   * at a table that's now free and showing someone else's stale order. */
  const confirmShift = async (toTableId: number) => {
    if (!shiftingTable) return;
    await shiftTableOrder(shiftingTable.id, toTableId);
    if (billingTableId === shiftingTable.id) {
      setTableId(toTableId);
    }
    setShiftingTable(null);
    reload();
  };

  return (
    <section className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Tables</h2>
          <p className="text-sm text-slate-500">Tap a table to start or resume its order.</p>
        </div>
        <button
          type="button"
          onClick={() => setShowAddForm((v) => !v)}
          className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-700 hover:bg-slate-50"
        >
          <Plus className="h-4 w-4" />
          Add table
        </button>
      </div>

      {showAddForm && <AddTableForm onAdd={handleAddTable} onCancel={() => setShowAddForm(false)} />}

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      {isLoading ? (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="h-24 animate-pulse rounded-lg border border-slate-200 bg-slate-50" />
          ))}
        </div>
      ) : tables.length === 0 ? (
        <p className="rounded-lg border border-dashed border-slate-300 px-4 py-10 text-center text-sm text-slate-400">
          No tables yet — add one to get started.
        </p>
      ) : (
        <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 lg:grid-cols-4">
          {tables.map((table) => (
            <TableCard
              key={table.id}
              table={table}
              busy={busyId === table.id}
              onSeat={() => void seatTable(table)}
              onResume={() => void resumeTable(table)}
              onRelease={() => void releaseTable(table)}
              onReserve={() => void reserveTable(table)}
              onShift={() => setShiftingTable(table)}
              onEdit={() => setEditingTable(table)}
              onDelete={() => {
                setDeleteError(null);
                setDeletingTable(table);
              }}
            />
          ))}
        </div>
      )}

      {shiftingTable && (
        <ShiftTableModal
          fromTable={shiftingTable}
          freeTables={tables.filter((t) => t.status === "free")}
          onClose={() => setShiftingTable(null)}
          onConfirm={confirmShift}
        />
      )}

      {editingTable && (
        <EditTableModal
          table={editingTable}
          onSave={handleRenameTable}
          onClose={() => setEditingTable(null)}
        />
      )}

      {deletingTable && (
        <ConfirmDialog
          title={`Delete ${deletingTable.name}?`}
          message={
            deleteError ??
            "This removes the table from the floor. It can't be undone, and only works while the table has no order in progress."
          }
          confirmLabel="Delete"
          isDangerous
          isBusy={isDeleting}
          onConfirm={() => void confirmDeleteTable()}
          onCancel={() => {
            setDeletingTable(null);
            setDeleteError(null);
          }}
        />
      )}
    </section>
  );
}
