import { ArrowRight, Pause, ShoppingBag, X } from "lucide-react";
import { useWorkspaceDataChanged } from "../hooks/useWorkspaceDataChanged";
import { useEffect, useRef, useState } from "react";
import { ActiveOrdersDrawer } from "../components/billing/ActiveOrdersDrawer";
import { BillingHeader } from "../components/billing/BillingHeader";
import { CartPanel } from "../components/billing/CartPanel";
import { CashTenderControl } from "../components/billing/CashTenderControl";
import { CategoryPills } from "../components/billing/CategoryPills";
import { DiscountControl } from "../components/billing/DiscountControl";
import { EditNoteModal } from "../components/billing/EditNoteModal";
import { HoldOrderModal } from "../components/billing/HoldOrderModal";
import { ItemAmountEntryModal } from "../components/billing/ItemAmountEntryModal";
import { ItemGrid } from "../components/billing/ItemGrid";
import { ItemSearchBar } from "../components/billing/ItemSearchBar";
import { MobileCategoryChips, type MobileCategorySelection } from "../components/billing/MobileCategoryChips";
import { OrderSummaryHeader } from "../components/billing/OrderSummaryHeader";
import { OrderTypeAndTable } from "../components/billing/OrderTypeAndTable";
import { PaymentMethodSelector } from "../components/billing/PaymentMethodSelector";
import { ReceiptModal } from "../components/billing/ReceiptModal";
import { ShortcutsHelpOverlay } from "../components/billing/ShortcutsHelpOverlay";
import { SplitPaymentEditor } from "../components/billing/SplitPaymentEditor";
import { TableQuickSelectModal } from "../components/billing/TableQuickSelectModal";
import { useBestSellerIds } from "../hooks/useBestSellerIds";
import { useFastBillingHotkeys } from "../hooks/useFastBillingHotkeys";
import { useModuleEnabled } from "../hooks/useModules";
import { createSale } from "../services/billingService";
import { getOrder, listActiveOrders, payOrder, transitionOrder } from "../services/ordersService";
import { getCategories, getItems } from "../services/inventoryService";
import { useAppConfig } from "../hooks/useAppConfig";
import { useBillingStore, useShiftStore } from "../store";
import { IS_ANDROID } from "../services/platform";
import { computeCartTotals } from "../utils/billingTotals";
import { buildCheckoutPayments } from "../utils/checkoutPayments";
import { decimalToMinor, formatMinor } from "../utils/format";
import { reportWarning } from "../services/logService";
import type { Category, Item, Sale } from "../types";
import { runAction } from "../utils/runAction";
import { DEFAULT_PREP_MINUTES, isDeliveryOrder } from "../utils/orderType";

export function BillingPage() {
  const { config } = useAppConfig();
  const tablesEnabled = useModuleEnabled("tables");
  const shiftsEnabled = useModuleEnabled("shifts");
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

  /** `quiet`: a background refresh (another counter changed the catalog) — no spinner. */
  const loadCatalog = (quiet = false) => {
    if (!quiet) setIsLoadingItems(true);
    setLoadError(null);
    Promise.all([getItems({}), getCategories()])
      .then(([loadedItems, loadedCategories]) => {
        setItems(loadedItems);
        setCategories(loadedCategories);
      })
      .catch((e: Error) => setLoadError(e.message))
      .finally(() => setIsLoadingItems(false));
  };

  useEffect(() => loadCatalog(), []);
  // Prices, stock and new products from the business's other counters.
  useWorkspaceDataChanged(() => loadCatalog(true));

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
  const activeOrderId = useBillingStore((state) => state.activeOrderId);
  const customerName = useBillingStore((state) => state.customerName);
  const tenderedAmount = useBillingStore((state) => state.tenderedAmount);
  const orderType = useBillingStore((state) => state.orderType);
  const deliveryDetails = useBillingStore((state) => state.deliveryDetails);
  const splitRows = useBillingStore((state) => state.splitRows);
  const dueDate = useBillingStore((state) => state.dueDate);
  const customer = useBillingStore((state) => state.customer);
  const taxExempt = useBillingStore((state) => state.taxExempt);
  const setTaxExempt = useBillingStore((state) => state.setTaxExempt);
  const serviceWaived = useBillingStore((state) => state.serviceWaived);
  const setServiceWaived = useBillingStore((state) => state.setServiceWaived);

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

  // Android only: the checkout column becomes a bottom sheet opened from a
  // floating cart button instead of a second column stacked under the item
  // grid — a phone-width screen doesn't have room to show both at once
  // without burying the grid below a scroll.
  const [isCartOpen, setIsCartOpen] = useState(false);

  // Active / Held Orders drawer & Hold Order modal
  const [activeOrdersCount, setActiveOrdersCount] = useState(0);
  const [hasLateOrders, setHasLateOrders] = useState(false);
  const [isOpenActiveOrders, setIsOpenActiveOrders] = useState(false);
  const [showHoldModal, setShowHoldModal] = useState(false);

  // Polled every 15s, so a lasting failure is logged once per streak.
  const activeOrdersFailing = useRef(false);
  const refreshActiveOrdersCount = async () => {
    try {
      const active = await listActiveOrders();
      setActiveOrdersCount(active.length);
      const currTime = Date.now();
      const anyLate = active.some((o) => {
        const created = new Date(o.createdAt).getTime();
        const elapsed = (currTime - created) / 60000;
        return elapsed > (o.estimatedPrepMinutes ?? DEFAULT_PREP_MINUTES);
      });
      setHasLateOrders(anyLate);
      activeOrdersFailing.current = false;
    } catch (e) {
      // Only the badge count depends on it — the till keeps working.
      if (!activeOrdersFailing.current) reportWarning(`billing: active orders count unavailable: ${(e as Error).message}`);
      activeOrdersFailing.current = true;
    }
  };

  useEffect(() => {
    void refreshActiveOrdersCount();
    const timer = window.setInterval(refreshActiveOrdersCount, 15000);
    return () => window.clearInterval(timer);
  }, []);

  // Keyboard fast billing (desktop only — Android stays touch-first) is
  // disabled while any modal already has the cashier's attention, so arrow
  // keys/Delete/quick-entry never fight with a field inside one of them.
  const fastBillingEnabled =
    !IS_ANDROID &&
    editingItemId === null &&
    completedSale === null &&
    amountEntryItem === null &&
    !isTokenDialogOpen &&
    !showHoldModal &&
    !isOpenActiveOrders;
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
  const deliveryChargeMinor =
    orderType === "delivery" && deliveryDetails?.deliveryChargeMinor
      ? deliveryDetails.deliveryChargeMinor
      : 0;
  const totals = computeCartTotals(
    cartLines,
    discountMode,
    discountValue,
    taxExempt ? 0 : config.taxPercent,
    deliveryChargeMinor,
    serviceWaived ? 0 : config.serviceChargePercent,
  );
  // What the server derives as the sale total — the delivery fee lives on
  // the order, not the sale, so tenders must add up to this, not the total.
  const payableMinor = totals.totalMinor - totals.deliveryChargeMinor;

  const completeSale = async () => {
    if (cartOrder.length === 0) return;
    // Set when the payment went through but a follow-up step didn't — shown after runAction clears the error line.
    let followUpError: string | null = null;
    const checkout = buildCheckoutPayments(paymentMethod, payableMinor, splitRows, config.currency);
    if (checkout.error) {
      setError(checkout.error);
      return;
    }
    if (checkout.creditMinor > 0 && !customer) {
      setError("Choose a customer before selling on credit.");
      return;
    }
    const tenders = {
      paymentMethod: checkout.paymentMethod,
      payments: checkout.payments,
      customerId: customer?.id ?? null,
      serviceChargeMinor: totals.serviceChargeMinor,
      dueDate: checkout.creditMinor > 0 ? dueDate : null,
    };
    const ok = await runAction(setIsSubmitting, setError, async () => {
      let sale: Sale;
      let tenderedMinor: number | null = null;
      let changeMinor: number | null = null;
      if (paymentMethod === "cash") {
        const tenderedVal = tenderedAmount !== null && tenderedAmount > 0
          ? decimalToMinor(tenderedAmount)
          : totals.totalMinor;
        tenderedMinor = tenderedVal;
        changeMinor = Math.max(0, tenderedVal - totals.totalMinor);
      }

      if (activeOrderId != null) {
        // Resumed active order settlement
        const currentOrder = await getOrder(activeOrderId);
        const unpaidActiveItemIds = currentOrder.lines
          .filter((l) => l.status === "active" && l.saleId === null)
          .map((l) => l.id);

        const paidResult = await payOrder({
          orderId: activeOrderId,
          version: currentOrder.version,
          orderItemIds: unpaidActiveItemIds,
          ...tenders,
          discountMinor: totals.discountMinor,
          taxMinor: totals.taxMinor,
          tenderedMinor,
          changeMinor,
        });

        sale = paidResult.sale;

        // Transition order to served (handed over) for takeaway / counter orders so they immediately complete
        if (!isDeliveryOrder(paidResult.order.orderType)) {
          if (paidResult.order.status !== "served" && !paidResult.order.completedAt) {
            try {
              await transitionOrder(paidResult.order.id, paidResult.order.version, "served");
            } catch (tErr) {
              followUpError = `Payment recorded, but order #${paidResult.order.orderNo} couldn't be marked served (${(tErr as Error).message}). Mark it served from Active Orders.`;
              reportWarning(`billing: transition to served failed for order ${paidResult.order.id}: ${(tErr as Error).message}`);
            }
          }
        }
      } else {
        sale = await createSale({
          items: cartOrder.map((id) => ({ itemId: id, qty: cart[id].qty, notes: cart[id].notes || undefined })),
          discountMinor: totals.discountMinor,
          taxMinor: totals.taxMinor,
          ...tenders,
          tableId,
          tenderedMinor,
          changeMinor,
        });
      }
      setCompletedSale(sale);
      clearCart();
      setIsCartOpen(false);
      loadCatalog(); // stock just changed — the grid should reflect it
      reloadBestSellers(); // this sale may have moved the best-sellers ranking
      void refreshActiveOrdersCount();
    });
    if (ok && followUpError) setError(followUpError);
  };

  return (
    <div className="billing-layout flex h-full flex-col gap-3.5 md:flex-row md:gap-4 lg:gap-5">
      {/* Browsing column */}
      <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 sm:gap-3.5 pb-20 md:pb-0 lg:pb-0">
        <BillingHeader
          onRefunded={() => loadCatalog()}
          onOpenActiveOrders={() => setIsOpenActiveOrders(true)}
          activeOrdersCount={activeOrdersCount}
          hasLateOrders={hasLateOrders}
        />
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

        {loadError && <p className="rounded-2xl bg-danger-soft px-4 py-2.5 text-sm text-danger-text">{loadError}</p>}
        {notice && <p className="rounded-2xl bg-success-soft px-4 py-2.5 text-sm text-success-text">{notice}</p>}

        <div className="flex items-center justify-between gap-3">
          <h3 className="text-sm font-semibold text-fg">{isSearching ? "Search results" : selectedCategoryId === null ? "All products" : selectedCategoryId === "best-seller" ? "Best sellers" : categories.find((category) => category.id === selectedCategoryId)?.name ?? "Products"}</h3>
          <span className="rounded-lg bg-surface px-2 py-1 text-[11px] text-fg-muted">{isLoadingItems ? "Loading…" : `${visibleItems.length} items`}</span>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-0.5 pb-2">
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

      {/* Checkout column — always on for desktop; on Android it's a bottom
       * sheet, only mounted while `isCartOpen`, opened from the floating
       * cart button below. */}
      {IS_ANDROID && isCartOpen && (
        <div className="fixed inset-0 z-40 bg-scrim" onClick={() => setIsCartOpen(false)} />
      )}
      {(!IS_ANDROID || isCartOpen) && (
      <div
        className={
          IS_ANDROID
            ? "fixed inset-x-0 bottom-0 z-50 flex max-h-[88vh] w-full flex-col gap-4 overflow-y-auto rounded-t-3xl border border-line bg-surface p-5 pb-[calc(1.25rem+env(safe-area-inset-bottom))] shadow-soft-lg"
            : "billing-checkout flex w-full shrink-0 flex-col gap-3 sm:gap-3.5 overflow-y-auto rounded-2xl border border-line bg-surface p-3.5 sm:p-4 shadow-soft md:w-80 lg:w-84 xl:w-92 2xl:w-100 h-full min-h-0"
        }
      >
        {IS_ANDROID && (
          <div className="flex items-center justify-between">
            <h3 className="text-base font-semibold text-fg">Your order</h3>
            <button
              type="button"
              onClick={() => setIsCartOpen(false)}
              className="rounded-lg p-1.5 text-fg-muted hover:bg-surface-2"
              aria-label="Close order"
            >
              <X className="h-5 w-5" />
            </button>
          </div>
        )}
        <OrderSummaryHeader currency={config.currency} />

        {activeOrderId && (
          <div className="flex items-center justify-between rounded-xl border border-accent/30 bg-accent-soft px-3.5 py-2 text-xs font-medium text-accent-text">
            <span className="flex items-center gap-1.5">
              <ShoppingBag className="h-3.5 w-3.5" />
              Settling Active Order {customerName ? `(${customerName})` : `#${activeOrderId}`}
            </span>
            <button
              type="button"
              onClick={() => {
                clearCart();
                void refreshActiveOrdersCount();
              }}
              className="text-[11px] font-semibold underline opacity-80 hover:opacity-100"
            >
              Cancel
            </button>
          </div>
        )}

        {tablesEnabled && (
          <OrderTypeAndTable
            taxPercent={config.taxPercent}
            currency={config.currency}
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

        <dl className="space-y-1.5 sm:space-y-2 rounded-xl border border-line bg-surface-2/40 p-2.5 sm:p-3 text-xs sm:text-sm tabular-nums">
          {/* Only when something sits between it and the total — otherwise it repeats the total. */}
          {totals.subtotalMinor !== totals.totalMinor && (
            <div className="flex justify-between text-fg-muted">
              <dt>Subtotal</dt>
              <dd>{formatMinor(totals.subtotalMinor, config.currency)}</dd>
            </div>
          )}
          {config.taxPercent > 0 && (
            <ChargeRow
              label={`Tax (${config.taxPercent}%)`}
              amount={formatMinor(totals.taxMinor, config.currency)}
              on={!taxExempt}
              onToggle={() => setTaxExempt(!taxExempt)}
            />
          )}
          {config.serviceChargePercent > 0 && (
            <ChargeRow
              label={`Service (${config.serviceChargePercent}%)`}
              amount={formatMinor(totals.serviceChargeMinor, config.currency)}
              on={!serviceWaived}
              onToggle={() => setServiceWaived(!serviceWaived)}
            />
          )}
          {totals.discountMinor > 0 && (
            <div className="flex justify-between text-success-text">
              <dt>Discount</dt>
              <dd>-{formatMinor(totals.discountMinor, config.currency)}</dd>
            </div>
          )}
          {totals.deliveryChargeMinor > 0 && (
            <div className="flex justify-between text-fg-muted">
              <dt>Delivery fee</dt>
              <dd>{formatMinor(totals.deliveryChargeMinor, config.currency)}</dd>
            </div>
          )}
          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-dashed border-line-strong pt-2 text-base sm:text-lg font-bold text-fg first:border-t-0 first:pt-0">
            <dt className="text-sm sm:text-base font-semibold text-fg">Total amount</dt>
            <dd className="text-xl sm:text-2xl font-bold tabular-nums text-fg">{formatMinor(totals.totalMinor, config.currency)}</dd>
          </div>
        </dl>

        <div className="flex flex-col gap-2.5 sm:gap-3">
          <DiscountControl />
          <PaymentMethodSelector />
          {paymentMethod === "cash" && (
            <CashTenderControl currency={config.currency} totalMinor={totals.totalMinor} />
          )}
          {(paymentMethod === "split" || paymentMethod === "credit") && (
            <SplitPaymentEditor currency={config.currency} payableMinor={payableMinor} />
          )}
        </div>

        {error && <p className="rounded-2xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setShowHoldModal(true)}
            disabled={cartOrder.length === 0 || isSubmitting}
            className="flex items-center justify-center gap-1.5 rounded-xl border border-line bg-surface-2 px-3.5 sm:px-4 py-3 sm:py-3.5 text-sm font-semibold text-fg hover:bg-surface-3 disabled:cursor-not-allowed disabled:opacity-40"
            title="Hold order with custom preparation time & clear cart for next customer"
          >
            <Pause className="h-4 w-4 text-accent-text" />
            <span>Hold</span>
          </button>

          <button
            type="button"
            onClick={() => void completeSale()}
            disabled={cartOrder.length === 0 || isSubmitting}
            className="flex min-w-0 flex-1 items-center justify-between gap-2 rounded-xl bg-accent px-3 sm:px-4 py-3 sm:py-3.5 text-sm font-semibold text-accent-ink transition-colors hover:bg-accent-hover disabled:cursor-not-allowed disabled:bg-surface-3 disabled:text-fg-muted"
          >
            <span className="whitespace-nowrap">{isSubmitting ? "Placing order…" : "Place order"}</span>
            <span className="flex items-center gap-2 whitespace-nowrap tabular-nums">{formatMinor(totals.totalMinor, config.currency)} <ArrowRight className="h-4 w-4" /></span>
          </button>
        </div>
      </div>
      )}

      {/* Floating cart button — Android only, hidden while the sheet above
       * is already open (its own header X closes it instead). No shadow-*
       * utility here: this WebView renders `shadow-soft-lg`'s large blurred
       * box-shadow as a hard rectangle instead of a soft blur, which looked
       * like a stray dark panel behind the pill. A ring is enough lift. */}
      {IS_ANDROID && !isCartOpen && (
        <button
          type="button"
          onClick={() => setIsCartOpen(true)}
          className="fixed bottom-20 right-4 z-30 flex items-center gap-2 rounded-full bg-accent px-4 py-3.5 text-accent-ink shadow-soft-lg"
        >
          <span className="relative flex items-center">
            <ShoppingBag className="h-5 w-5" />
            {cartOrder.length > 0 && (
              <span className="absolute -right-2 -top-2 flex h-4 min-w-4 items-center justify-center rounded-full bg-accent-ink px-1 text-[10px] font-bold text-accent">
                {cartOrder.length}
              </span>
            )}
          </span>
          <span className="text-sm font-semibold tabular-nums">{formatMinor(totals.totalMinor, config.currency)}</span>
        </button>
      )}

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
          className="pointer-events-none fixed bottom-5 right-5 z-40 rounded-xl bg-overlay px-3 py-1.5 font-mono text-sm text-overlay-ink shadow-soft-lg"
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

      {showHoldModal && (
        <HoldOrderModal
          currency={config.currency}
          totalMinor={totals.totalMinor}
          onClose={() => setShowHoldModal(false)}
          onOrderHeld={(order, tokenError) => {
            setNotice(`Order #${order.orderNo} held (${order.estimatedPrepMinutes}m target). Cart cleared.`);
            window.setTimeout(() => setNotice(null), 5000);
            if (tokenError) setError(`Kitchen token for order #${order.orderNo} didn't print (${tokenError}). Reprint it from Active Orders.`);
            void refreshActiveOrdersCount();
          }}
        />
      )}

      <ActiveOrdersDrawer
        isOpen={isOpenActiveOrders}
        onClose={() => {
          setIsOpenActiveOrders(false);
          void refreshActiveOrdersCount();
        }}
        currency={config.currency}
        onOrderResumed={(order) => {
          setNotice(`Resumed order #${order.orderNo} into cart.`);
          window.setTimeout(() => setNotice(null), 4000);
          void refreshActiveOrdersCount();
        }}
      />
    </div>
  );
}

/** A tax/service-charge line in the totals box with an on/off switch for
 * this sale only (a tax-exempt customer, a waived service charge). */
function ChargeRow({ label, amount, on, onToggle }: { label: string; amount: string; on: boolean; onToggle: () => void }) {
  return (
    <div className="flex items-center justify-between gap-2 text-fg-muted">
      <dt className="flex items-center gap-2">
        <button
          type="button"
          role="switch"
          aria-checked={on}
          aria-label={`${on ? "Remove" : "Apply"} ${label}`}
          onClick={onToggle}
          className={`relative h-4 w-7 shrink-0 rounded-full transition-colors ${on ? "bg-accent" : "bg-surface-3"}`}
        >
          <span className={`absolute top-0.5 h-3 w-3 rounded-full bg-surface transition-all ${on ? "left-3.5" : "left-0.5"}`} />
        </button>
        <span className={on ? "" : "line-through"}>{label}</span>
      </dt>
      <dd>{on ? amount : "—"}</dd>
    </div>
  );
}
