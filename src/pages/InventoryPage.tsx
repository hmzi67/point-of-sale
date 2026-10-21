import { Boxes, LockKeyhole, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { CsvImportModal } from "../components/inventory/CsvImportModal";
import { EMPTY_INVENTORY_FILTERS, InventoryFilterChips } from "../components/inventory/InventoryFilterChips";
import { InventorySnapshot } from "../components/inventory/InventorySnapshot";
import { InventoryToolbar } from "../components/inventory/InventoryToolbar";
import { ItemFormModal } from "../components/inventory/ItemFormModal";
import { ItemTable } from "../components/inventory/ItemTable";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { useAppConfig } from "../hooks/useAppConfig";
import { useBestSellerIds } from "../hooks/useBestSellerIds";
import { useAuthStore, useInventoryStore } from "../store";
import { minorToDecimal } from "../utils/format";
import { isModuleReadOnlyFor } from "../utils/permissions";
import type { Item } from "../types";
import { runAction } from "../utils/runAction";

export function InventoryPage() {
  const role = useAuthStore((state) => state.user?.role ?? "cashier");
  const isReadOnly = isModuleReadOnlyFor(role, "inventory");

  const items = useInventoryStore((state) => state.items);
  const isLoading = useInventoryStore((state) => state.isLoading);
  const error = useInventoryStore((state) => state.error);
  const load = useInventoryStore((state) => state.load);
  const deleteItem = useInventoryStore((state) => state.deleteItem);
  const deleteItems = useInventoryStore((state) => state.deleteItems);
  const { config } = useAppConfig();

  const [editingItem, setEditingItem] = useState<Item | undefined>(undefined);
  const [isAdding, setIsAdding] = useState(false);
  const [isImportingCsv, setIsImportingCsv] = useState(false);
  const [deletingItem, setDeletingItem] = useState<Item | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [isBulkDeleting, setIsBulkDeleting] = useState(false);
  const [isConfirmingBulkDelete, setIsConfirmingBulkDelete] = useState(false);
  const [bulkDeleteError, setBulkDeleteError] = useState<string | null>(null);

  const [filters, setFilters] = useState(EMPTY_INVENTORY_FILTERS);
  // Recomputed live every time this page loads — never a stored flag — so
  // the fire badge/filter reflects whatever actually sold most recently.
  const { bestSellerIds } = useBestSellerIds();

  useEffect(() => {
    void load();
  }, [load]);

  // Chips AND together with each other and with the toolbar's existing
  // search/category filter (already applied server-side, upstream in `items`).
  const filteredItems = useMemo(() => {
    const minPrice = filters.minPrice === "" ? null : Number(filters.minPrice);
    const maxPrice = filters.maxPrice === "" ? null : Number(filters.maxPrice);
    return items.filter((item) => {
      if (filters.lowStock && !item.isLowStock) return false;
      if (filters.outOfStock && item.stockQty > 0) return false;
      if (filters.bestSeller && !bestSellerIds.has(item.id)) return false;
      const priceDecimal = minorToDecimal(item.priceMinor);
      if (minPrice !== null && !Number.isNaN(minPrice) && priceDecimal < minPrice) return false;
      if (maxPrice !== null && !Number.isNaN(maxPrice) && priceDecimal > maxPrice) return false;
      return true;
    });
  }, [items, filters, bestSellerIds]);

  // Drop any selected id that no longer appears in the current list — e.g.
  // after a search/category/filter-chip change, or once a bulk delete
  // completes. Keyed off `filteredItems`, not `items`, so a chip narrowing
  // the visible rows also drops selections that scrolled out of view.
  useEffect(() => {
    setSelectedIds((prev) => {
      const itemIds = new Set(filteredItems.map((item) => item.id));
      const next = new Set([...prev].filter((id) => itemIds.has(id)));
      return next.size === prev.size ? prev : next;
    });
  }, [filteredItems]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 3000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const confirmDelete = async () => {
    if (!deletingItem) return;
    await runAction(setIsDeleting, setDeleteError, async () => {
      const outcome = await deleteItem(deletingItem.id);
      setToast(
        outcome === "archived"
          ? `${deletingItem.name} has sale history, so it was archived instead of deleted.`
          : `${deletingItem.name} was deleted.`,
      );
      setDeletingItem(null);
    });
  };

  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    setSelectedIds((prev) =>
      prev.size === filteredItems.length ? new Set() : new Set(filteredItems.map((item) => item.id)),
    );
  };

  const confirmBulkDelete = async () => {
    await runAction(setIsBulkDeleting, setBulkDeleteError, async () => {
      const ids = [...selectedIds];
      const { deleted, archived, failed } = await deleteItems(ids);
      const parts = [];
      if (deleted > 0) parts.push(`${deleted} deleted`);
      if (archived > 0) parts.push(`${archived} archived (has sale history)`);
      if (failed.length > 0) parts.push(`${failed.length} failed`);
      setToast(parts.join(", ") + ".");
      setSelectedIds(new Set());
      setIsConfirmingBulkDelete(false);
    });
  };

  return (
    <section className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3 py-1">
        <div>
          <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Product management</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Inventory</h2>
          <p className="mt-2 text-sm text-fg-muted">Keep your products organized and your shelves ready.</p>
        </div>
        {isReadOnly && <span className="flex items-center gap-2 rounded-xl border border-line bg-surface px-3 py-2 text-xs text-fg-muted"><LockKeyhole className="h-3.5 w-3.5" />Read-only access</span>}
      </div>

      <InventorySnapshot items={filteredItems} isLoading={isLoading} />

      <div className="space-y-4 rounded-2xl border border-line bg-surface p-4 sm:p-5">
      <InventoryToolbar
        isReadOnly={isReadOnly}
        onAddItem={() => setIsAdding(true)}
        onImportCsv={() => setIsImportingCsv(true)}
      />

      <div className="border-t border-line pt-4">
        <InventoryFilterChips filters={filters} onChange={setFilters} />
      </div>
      </div>

      {!isReadOnly && selectedIds.size > 0 && (
        <div className="flex items-center justify-between rounded-xl border border-accent/40 bg-accent-soft px-4 py-2.5">
          <span className="text-sm font-medium text-accent-text">
            {selectedIds.size} item{selectedIds.size === 1 ? "" : "s"} selected
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setSelectedIds(new Set())}
              className="flex items-center gap-1 rounded-xl px-2.5 py-1.5 text-sm font-medium text-accent-text hover:bg-accent-soft"
            >
              <X className="h-4 w-4" />
              Clear
            </button>
            <button
              type="button"
              onClick={() => setIsConfirmingBulkDelete(true)}
              className="flex items-center gap-1.5 rounded-xl bg-danger px-3 py-1.5 text-sm font-medium text-white hover:bg-danger/85"
            >
              <Trash2 className="h-4 w-4" />
              Delete selected
            </button>
          </div>
        </div>
      )}

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}
      {toast && <p className="rounded-xl bg-success-soft px-3 py-2 text-sm text-success-text">{toast}</p>}

      <div className="space-y-3">
        <div className="flex items-center justify-between gap-3 px-1">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-fg"><Boxes className="h-4 w-4 text-accent-text" />Product catalog</h3>
          <span className="text-xs text-fg-muted">{isLoading ? "Updating…" : `${filteredItems.length} products shown`}</span>
        </div>
      {isLoading && items.length === 0 ? (
        <div role="status" aria-label="Loading products" className="h-80 animate-pulse rounded-2xl border border-line bg-surface" />
      ) : (
        <ItemTable
          items={filteredItems}
          isReadOnly={isReadOnly}
          currency={config.currency}
          onEdit={setEditingItem}
          onDelete={setDeletingItem}
          bestSellerIds={bestSellerIds}
          selectedIds={selectedIds}
          onToggleSelect={toggleSelect}
          onToggleSelectAll={toggleSelectAll}
        />
      )}

      </div>

      {isImportingCsv && (
        <CsvImportModal onClose={() => setIsImportingCsv(false)} onImported={() => void load()} />
      )}

      {(isAdding || editingItem) && (
        <ItemFormModal
          item={editingItem}
          onClose={() => {
            setIsAdding(false);
            setEditingItem(undefined);
          }}
        />
      )}

      {deletingItem && (
        <ConfirmDialog
          title={`Delete ${deletingItem.name}?`}
          message={
            deleteError ??
            "This removes the item from inventory. If it has sale history it will be archived instead, so past reports stay accurate."
          }
          confirmLabel="Delete"
          isDangerous
          isBusy={isDeleting}
          onConfirm={() => void confirmDelete()}
          onCancel={() => {
            setDeletingItem(null);
            setDeleteError(null);
          }}
        />
      )}

      {isConfirmingBulkDelete && (
        <ConfirmDialog
          title={`Delete ${selectedIds.size} item${selectedIds.size === 1 ? "" : "s"}?`}
          message={
            bulkDeleteError ??
            "Items with sale history will be archived instead of deleted, so past reports stay accurate."
          }
          confirmLabel="Delete"
          isDangerous
          isBusy={isBulkDeleting}
          onConfirm={() => void confirmBulkDelete()}
          onCancel={() => {
            setIsConfirmingBulkDelete(false);
            setBulkDeleteError(null);
          }}
        />
      )}
    </section>
  );
}
