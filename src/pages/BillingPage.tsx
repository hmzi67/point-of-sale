import { useEffect, useState } from "react";
import { BillingHeader } from "../components/billing/BillingHeader";
import { CartPanel } from "../components/billing/CartPanel";
import { CategoryPills } from "../components/billing/CategoryPills";
import { DiscountControl } from "../components/billing/DiscountControl";
import { ItemDetailModal } from "../components/billing/ItemDetailModal";
import { ItemGrid } from "../components/billing/ItemGrid";
import { ItemSearchBar } from "../components/billing/ItemSearchBar";
import { OrderSummaryHeader } from "../components/billing/OrderSummaryHeader";
import { OrderTypeAndTable } from "../components/billing/OrderTypeAndTable";
import { PaymentMethodSelector } from "../components/billing/PaymentMethodSelector";
import { ReceiptModal } from "../components/billing/ReceiptModal";
import { useModules } from "../hooks/useModules";
import { createSale } from "../services/billingService";
import { getCategories, getItems } from "../services/inventoryService";
import { useAppConfig } from "../hooks/useAppConfig";
import { useAuthStore, useBillingStore } from "../store";
import { computeCartTotals } from "../utils/billingTotals";
import { formatMinor } from "../utils/format";
import type { Category, Item, Sale } from "../types";

export function BillingPage() {
  const { config } = useAppConfig();
  const cashierId = useAuthStore((state) => state.user?.id ?? null);
  const { modules } = useModules();
  const tablesEnabled = modules.some((m) => m.key === "tables" && m.enabled);

  // --- Item browsing (grid + category pills) --------------------------------
  const [items, setItems] = useState<Item[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [selectedCategoryId, setSelectedCategoryId] = useState<number | null>(null);
  const [isLoadingItems, setIsLoadingItems] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadCatalog = () => {
    setIsLoadingItems(true);
    setLoadError(null);
    Promise.all([getItems({}), getCategories()])
      .then(([loadedItems, loadedCategories]) => {
        setItems(loadedItems);
        setCategories(loadedCategories);
      })
      .catch((e: Error) => setLoadError(e.message))
      .finally(() => setIsLoadingItems(false));
  };

  useEffect(loadCatalog, []);

  const visibleItems =
    selectedCategoryId === null ? items : items.filter((item) => item.categoryId === selectedCategoryId);

  // --- Item detail modal: add-mode (grid tap) and edit-mode (cart pencil) ---
  const [openItem, setOpenItem] = useState<Item | null>(null);
  const [editingItemId, setEditingItemId] = useState<number | null>(null);

  const cart = useBillingStore((state) => state.cart);
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const tableId = useBillingStore((state) => state.tableId);
  const clearCart = useBillingStore((state) => state.clearCart);
  const addItemWithDetails = useBillingStore((state) => state.addItemWithDetails);
  const updateLine = useBillingStore((state) => state.updateLine);

  const editingEntry = editingItemId !== null ? cart[editingItemId] : undefined;
  const editingItem = editingItemId !== null ? items.find((i) => i.id === editingItemId) : undefined;

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [completedSale, setCompletedSale] = useState<Sale | null>(null);

  const cartLines = cartOrder.map((id) => cart[id]);
  const totals = computeCartTotals(cartLines, discountMode, discountValue, config.taxPercent);

  const completeSale = async () => {
    if (cartOrder.length === 0) return;
    setIsSubmitting(true);
    setError(null);
    try {
      const sale = await createSale({
        items: cartOrder.map((id) => ({ itemId: id, qty: cart[id].qty, notes: cart[id].notes || undefined })),
        discountMinor: totals.discountMinor,
        taxMinor: totals.taxMinor,
        paymentMethod,
        cashierId,
        tableId,
      });
      setCompletedSale(sale);
      clearCart();
      loadCatalog(); // stock just changed — the grid should reflect it
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="flex h-full flex-col gap-4 lg:flex-row lg:gap-6">
      {/* Browsing column */}
      <div className="flex min-w-0 flex-1 flex-col gap-4">
        <BillingHeader />
        <CategoryPills
          categories={categories}
          items={items}
          selectedCategoryId={selectedCategoryId}
          onSelect={setSelectedCategoryId}
        />
        <ItemSearchBar currency={config.currency} />

        {loadError && <p className="rounded-2xl bg-red-50 px-4 py-2.5 text-sm text-red-700">{loadError}</p>}
        {notice && <p className="rounded-2xl bg-emerald-50 px-4 py-2.5 text-sm text-emerald-700">{notice}</p>}

        <div className="flex-1 overflow-y-auto pb-2">
          <ItemGrid items={visibleItems} currency={config.currency} isLoading={isLoadingItems} onOpenItem={setOpenItem} />
        </div>
      </div>

      {/* Checkout column */}
      <div className="flex w-full shrink-0 flex-col gap-4 overflow-y-auto rounded-3xl bg-white p-5 shadow-soft lg:w-[380px]">
        <OrderSummaryHeader />

        {tablesEnabled && (
          <OrderTypeAndTable
            taxPercent={config.taxPercent}
            onParked={(message) => {
              setNotice(message);
              window.setTimeout(() => setNotice(null), 4000);
            }}
          />
        )}

        <div className="flex flex-1 flex-col overflow-hidden">
          <CartPanel currency={config.currency} onEditLine={setEditingItemId} />
        </div>

        <dl className="space-y-1.5 border-t border-slate-100 pt-3 text-sm">
          <div className="flex justify-between text-slate-500">
            <dt>Subtotal</dt>
            <dd>{formatMinor(totals.subtotalMinor, config.currency)}</dd>
          </div>
          {config.taxPercent > 0 && (
            <div className="flex justify-between text-slate-500">
              <dt>Tax ({config.taxPercent}%)</dt>
              <dd>{formatMinor(totals.taxMinor, config.currency)}</dd>
            </div>
          )}
          {totals.discountMinor > 0 && (
            <div className="flex justify-between text-emerald-600">
              <dt>Discount</dt>
              <dd>-{formatMinor(totals.discountMinor, config.currency)}</dd>
            </div>
          )}
          <div className="flex justify-between border-t border-slate-100 pt-1.5 text-base font-bold text-slate-900">
            <dt>TOTAL</dt>
            <dd>{formatMinor(totals.totalMinor, config.currency)}</dd>
          </div>
        </dl>

        <div className="flex gap-2">
          <div className="flex-[1.3]">
            <DiscountControl />
          </div>
          <div className="flex-1">
            <PaymentMethodSelector />
          </div>
        </div>

        {error && <p className="rounded-2xl bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

        <button
          type="button"
          onClick={() => void completeSale()}
          disabled={cartOrder.length === 0 || isSubmitting}
          className="rounded-2xl bg-brand-600 py-3.5 text-base font-semibold text-white shadow-soft hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSubmitting ? "Placing order…" : `Place Order · ${formatMinor(totals.totalMinor, config.currency)}`}
        </button>
      </div>

      {openItem && (
        <ItemDetailModal
          item={openItem}
          currency={config.currency}
          onClose={() => setOpenItem(null)}
          onConfirm={(qty, notes) => {
            addItemWithDetails(openItem, qty, notes);
            setOpenItem(null);
          }}
        />
      )}

      {editingItem && editingEntry && (
        <ItemDetailModal
          item={editingItem}
          currency={config.currency}
          initialQty={editingEntry.qty}
          initialNotes={editingEntry.notes}
          confirmLabel="Update Cart"
          onClose={() => setEditingItemId(null)}
          onConfirm={(qty, notes) => {
            updateLine(editingItem.id, qty, notes);
            setEditingItemId(null);
          }}
        />
      )}

      {completedSale && (
        <ReceiptModal sale={completedSale} config={config} onClose={() => setCompletedSale(null)} />
      )}
    </div>
  );
}
