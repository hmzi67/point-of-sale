import { useEffect, useState } from "react";
import { BillingHeader } from "../components/billing/BillingHeader";
import { CartPanel } from "../components/billing/CartPanel";
import { CategoryPills } from "../components/billing/CategoryPills";
import { DiscountControl } from "../components/billing/DiscountControl";
import { EditNoteModal } from "../components/billing/EditNoteModal";
import { ItemAmountEntryModal } from "../components/billing/ItemAmountEntryModal";
import { ItemGrid } from "../components/billing/ItemGrid";
import { ItemSearchBar } from "../components/billing/ItemSearchBar";
import { MobileCategoryChips, type MobileCategorySelection } from "../components/billing/MobileCategoryChips";
import { OrderSummaryHeader } from "../components/billing/OrderSummaryHeader";
import { OrderTypeAndTable } from "../components/billing/OrderTypeAndTable";
import { PaymentMethodSelector } from "../components/billing/PaymentMethodSelector";
import { ReceiptModal } from "../components/billing/ReceiptModal";
import { ShortcutsHelpOverlay } from "../components/billing/ShortcutsHelpOverlay";
import { TableQuickSelectModal } from "../components/billing/TableQuickSelectModal";
import { useBestSellerIds } from "../hooks/useBestSellerIds";
import { useFastBillingHotkeys } from "../hooks/useFastBillingHotkeys";
import { useModules } from "../hooks/useModules";
import { createSale } from "../services/billingService";
import { getCategories, getItems } from "../services/inventoryService";
import { useAppConfig } from "../hooks/useAppConfig";
import { useAuthStore, useBillingStore, useShiftStore } from "../store";
import { IS_ANDROID } from "../types";
import { computeCartTotals } from "../utils/billingTotals";
import { formatMinor } from "../utils/format";
import type { Category, Item, Sale } from "../types";

export function BillingPage() {
  const { config } = useAppConfig();
  const cashierId = useAuthStore((state) => state.user?.id ?? null);
  const { modules } = useModules();
  const tablesEnabled = modules.some((m) => m.key === "tables" && m.enabled);
  const shiftsEnabled = modules.some((m) => m.key === "shifts" && m.enabled);
  const openShiftId = useShiftStore((state) => state.openShift?.id ?? null);
  const loadShift = useShiftStore((state) => state.load);

  useEffect(() => {
    if (shiftsEnabled) void loadShift();
  }, [shiftsEnabled, loadShift]);

  // --- Item browsing (grid + category pills) --------------------------------
  const [items, setItems] = useState<Item[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  // `"best-seller"` is Android-only (see `MobileCategoryChips`) — desktop's
  // `CategoryPills` never sets it, so `visibleItems` below only ever needs
  // to check for it, never worry about it appearing on desktop.
  const [selectedCategoryId, setSelectedCategoryId] = useState<MobileCategorySelection>(null);
  const [isLoadingItems, setIsLoadingItems] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  // Search (from ItemSearchBar) always queries the whole catalog, never
  // scoped to `selectedCategoryId`. Non-empty `searchQuery` means the grid
  // shows `searchResults` instead of the category-filtered view below —
  // clearing the search box (searchQuery === "") reverts to whichever
  // category pill is selected, without needing to remember/restore anything.
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Item[]>([]);
  const isSearching = searchQuery.trim().length > 0;

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

  // Recomputed live on every catalog load (including right after a sale
  // completes, via loadCatalog below) — never cached across screens or
  // stored as a flag, so the fire badge can't go stale.
  const { bestSellerIds, reloadBestSellers } = useBestSellerIds();

  const visibleItems = isSearching
    ? searchResults
    : selectedCategoryId === null
      ? items
      : selectedCategoryId === "best-seller"
        ? items.filter((item) => bestSellerIds.has(item.id))
        : items.filter((item) => item.categoryId === selectedCategoryId);

  // --- Cart line note editing (the cart row's pencil icon) ------------------
  const [editingItemId, setEditingItemId] = useState<number | null>(null);

  const cart = useBillingStore((state) => state.cart);
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const tableId = useBillingStore((state) => state.tableId);
  const clearCart = useBillingStore((state) => state.clearCart);
  const updateLine = useBillingStore((state) => state.updateLine);
  const amountEntryItem = useBillingStore((state) => state.amountEntryItem);

  const editingEntry = editingItemId !== null ? cart[editingItemId] : undefined;

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [completedSale, setCompletedSale] = useState<Sale | null>(null);

  // Bumped on every Ctrl/Cmd+K; `isTokenDialogOpen` mirrors whether
  // `OrderTypeAndTable`'s "Print Token" dialog is currently up — see both
  // props' doc comments on `OrderTypeAndTable`.
  const [tokenPrintRequestId, setTokenPrintRequestId] = useState(0);
  const [isTokenDialogOpen, setIsTokenDialogOpen] = useState(false);

  // Keyboard fast billing (desktop only — Android stays touch-first) is
  // disabled while any modal already has the cashier's attention, so arrow
  // keys/Delete/quick-entry never fight with a field inside one of them.
  const fastBillingEnabled =
    !IS_ANDROID &&
    editingItemId === null &&
    completedSale === null &&
    amountEntryItem === null &&
    !isTokenDialogOpen;
  const fastBilling = useFastBillingHotkeys({
    enabled: fastBillingEnabled,
    items,
    tablesEnabled,
    tableId,
    // `completeSale` is defined further down this component, but this
    // wrapper isn't invoked until a real Ctrl/Cmd+Enter keypress — well
    // after render — so the forward reference is safe.
    onPlaceOrder: () => void completeSale(),
    onPrintToken: () => setTokenPrintRequestId((n) => n + 1),
  });

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
        shiftId: shiftsEnabled ? openShiftId : null,
      });
      setCompletedSale(sale);
      clearCart();
      loadCatalog(); // stock just changed — the grid should reflect it
      reloadBestSellers(); // this sale may have moved the best-sellers ranking
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
        <BillingHeader onRefunded={loadCatalog} />
        {IS_ANDROID ? (
          <MobileCategoryChips categories={categories} selected={selectedCategoryId} onSelect={setSelectedCategoryId} />
        ) : (
          <CategoryPills
            categories={categories}
            items={items}
            selectedCategoryId={selectedCategoryId === "best-seller" ? null : selectedCategoryId}
            onSelect={setSelectedCategoryId}
          />
        )}
        <ItemSearchBar
          onResultsChange={(query, results) => {
            setSearchQuery(query);
            setSearchResults(results);
          }}
        />

        {loadError && <p className="rounded-2xl bg-red-50 px-4 py-2.5 text-sm text-red-700">{loadError}</p>}
        {notice && <p className="rounded-2xl bg-emerald-50 px-4 py-2.5 text-sm text-emerald-700">{notice}</p>}

        <div className="flex-1 overflow-y-auto pb-2">
          <ItemGrid
            items={visibleItems}
            currency={config.currency}
            isLoading={isLoadingItems}
            isSearching={isSearching}
            bestSellerIds={bestSellerIds}
            fixedTwoColumns={IS_ANDROID}
          />
        </div>
      </div>

      {/* Checkout column */}
      <div className="flex w-full shrink-0 flex-col gap-4 overflow-y-auto rounded-3xl bg-white p-5 shadow-soft lg:w-110">
        <OrderSummaryHeader />

        {tablesEnabled && (
          <OrderTypeAndTable
            taxPercent={config.taxPercent}
            onParked={(message) => {
              setNotice(message);
              window.setTimeout(() => setNotice(null), 4000);
            }}
            tokenPrintRequestId={tokenPrintRequestId}
            onTokenDialogOpenChange={setIsTokenDialogOpen}
          />
        )}

        {/* CartPanel is the checkout column's flexible middle (it carries its
         * own flex-1 min-h-0 — see its doc comment): it claims exactly the
         * space left over between this header/table row above and the
         * totals/discount/payment/Place Order footer below, so that footer
         * always sits flush against the panel's bottom edge regardless of
         * cart size, instead of trailing right under a short item list. */}
        <CartPanel currency={config.currency} onEditLine={setEditingItemId} />

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

        <div className="flex flex-col gap-2">
          <DiscountControl />
          <PaymentMethodSelector />
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

      {editingItemId !== null && editingEntry && (
        <EditNoteModal
          itemName={editingEntry.name}
          initialNotes={editingEntry.notes}
          amountEntry={
            editingEntry.soldByAmount
              ? {
                  qty: editingEntry.qty,
                  unit: editingEntry.unit,
                  priceMinor: editingEntry.priceMinor,
                  stockQty: editingEntry.stockQty,
                }
              : undefined
          }
          onClose={() => setEditingItemId(null)}
          onSave={(notes, qty) => {
            updateLine(editingItemId, qty ?? editingEntry.qty, notes);
            setEditingItemId(null);
          }}
        />
      )}

      {completedSale && (
        <ReceiptModal
          sale={completedSale}
          config={config}
          tablesEnabled={tablesEnabled}
          onClose={() => setCompletedSale(null)}
        />
      )}

      <ItemAmountEntryModal />

      {/* Keyboard fast billing UI — see `useFastBillingHotkeys`. */}
      {fastBilling.buffer && (
        <div
          className="pointer-events-none fixed bottom-5 right-5 z-40 rounded-xl bg-slate-900/85 px-3 py-1.5 font-mono text-sm text-white shadow-soft-lg"
          aria-live="polite"
        >
          {fastBilling.buffer}
          <span className="animate-pulse">_</span>
        </div>
      )}
      {fastBilling.tablePopupOpen && <TableQuickSelectModal onClose={fastBilling.closeTablePopup} />}
      {fastBilling.showHelp && (
        <ShortcutsHelpOverlay tablesEnabled={tablesEnabled} onClose={fastBilling.closeHelp} />
      )}
    </div>
  );
}
