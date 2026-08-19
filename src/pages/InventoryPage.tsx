import { useEffect, useState } from "react";
import { CsvImportModal } from "../components/inventory/CsvImportModal";
import { InventoryToolbar } from "../components/inventory/InventoryToolbar";
import { ItemFormModal } from "../components/inventory/ItemFormModal";
import { ItemTable } from "../components/inventory/ItemTable";
import { ConfirmDialog } from "../components/ui/ConfirmDialog";
import { useAppConfig } from "../hooks/useAppConfig";
import { useAuthStore, useInventoryStore } from "../store";
import { isModuleReadOnlyFor } from "../utils/permissions";
import type { Item } from "../types";

export function InventoryPage() {
  const role = useAuthStore((state) => state.user?.role ?? "cashier");
  const isReadOnly = isModuleReadOnlyFor(role, "inventory");

  const items = useInventoryStore((state) => state.items);
  const isLoading = useInventoryStore((state) => state.isLoading);
  const error = useInventoryStore((state) => state.error);
  const load = useInventoryStore((state) => state.load);
  const deleteItem = useInventoryStore((state) => state.deleteItem);
  const { config } = useAppConfig();

  const [editingItem, setEditingItem] = useState<Item | undefined>(undefined);
  const [isAdding, setIsAdding] = useState(false);
  const [isImportingCsv, setIsImportingCsv] = useState(false);
  const [deletingItem, setDeletingItem] = useState<Item | null>(null);
  const [isDeleting, setIsDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), 3000);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const confirmDelete = async () => {
    if (!deletingItem) return;
    setIsDeleting(true);
    setDeleteError(null);
    try {
      const outcome = await deleteItem(deletingItem.id);
      setToast(
        outcome === "archived"
          ? `${deletingItem.name} has sale history, so it was archived instead of deleted.`
          : `${deletingItem.name} was deleted.`,
      );
      setDeletingItem(null);
    } catch (e) {
      setDeleteError((e as Error).message);
    } finally {
      setIsDeleting(false);
    }
  };

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-slate-900">Inventory</h2>
          <p className="text-sm text-slate-500">
            {items.length} item{items.length === 1 ? "" : "s"}
            {isReadOnly && " · read-only for your role"}
          </p>
        </div>
      </div>

      <InventoryToolbar
        isReadOnly={isReadOnly}
        onAddItem={() => setIsAdding(true)}
        onImportCsv={() => setIsImportingCsv(true)}
      />

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}
      {toast && <p className="rounded-md bg-emerald-50 px-3 py-2 text-sm text-emerald-700">{toast}</p>}

      {isLoading && items.length === 0 ? (
        <p className="py-16 text-center text-sm text-slate-500">Loading items…</p>
      ) : (
        <ItemTable
          items={items}
          isReadOnly={isReadOnly}
          currency={config.currency}
          onEdit={setEditingItem}
          onDelete={setDeletingItem}
        />
      )}

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
    </section>
  );
}
