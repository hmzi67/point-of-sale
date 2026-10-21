import { useEffect, useRef, useState } from "react";
import { Bike, Edit2, Store, Ticket } from "lucide-react";
import { getItems } from "../../services/inventoryService";
import { attachCartToTable, getParkedCart, getTables } from "../../services/tablesService";
import { useBillingStore } from "../../store";
import { useModuleEnabled } from "../../hooks/useModules";
import { computeCartTotals } from "../../utils/billingTotals";
import { DropdownSelect } from "../ui/DropdownSelect";
import { TokenPrintDialog, type TokenPrintSource } from "./TokenPrintDialog";
import { DeliveryDetailsModal } from "./DeliveryDetailsModal";
import { ThirdPartyDeliveryModal } from "./ThirdPartyDeliveryModal";
import type { TableSummary } from "../../types";
import { runAction } from "../../utils/runAction";
import { ORDER_TYPE_LABELS } from "../../utils/orderType";

interface OrderTypeAndTableProps {
  taxPercent: number;
  /** Passed straight through to `TokenPrintDialog` for its sold-by-amount
   * lines' bracketed rupee amount. */
  currency: string;
  onParked: (message: string) => void;
  /** Bumped by `BillingPage` on every Ctrl/Cmd+K press (see
   * `useFastBillingHotkeys`) — this component owns the actual "Print Token"
   * flow (table sync, `openOrderId` lookup, dialog state), so the keyboard
   * shortcut just re-triggers the same `openTableTokenDialog` the mouse
   * button calls rather than duplicating any of that logic upstream. The
   * initial value is never acted on — only a *change* opens the dialog. */
  tokenPrintRequestId: number;
  /** Reported every time the "Print Token" dialog opens/closes, so
   * `BillingPage` can suspend the rest of the keyboard-fast-billing surface
   * (digits, arrows, Ctrl+Enter, …) while it's up — mirrors how
   * `editingItemId`/`completedSale`/`amountEntryItem` already gate
   * `fastBillingEnabled` there. */
  onTokenDialogOpenChange: (open: boolean) => void;
}

/** Same free/occupied/reserved color convention as the floor view's
 * `TableCard` — a small dot next to each table option, restyled for a light
 * background instead of losing the status info in the redesign. */
const STATUS_DOT: Record<TableSummary["status"], string> = {
  free: "bg-success",
  occupied: "bg-danger",
  reserved: "bg-warning",
};

const STATUS_LABEL: Record<TableSummary["status"], string> = {
  free: "Free",
  occupied: "Occupied",
  reserved: "Reserved",
};

export function OrderTypeAndTable({
  taxPercent,
  currency,
  onParked,
  tokenPrintRequestId,
  onTokenDialogOpenChange,
}: OrderTypeAndTableProps) {
  const [tables, setTables] = useState<TableSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isBusy, setIsBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tokenSource, setTokenSource] = useState<TokenPrintSource | null>(null);
  const [isDeliveryModalOpen, setIsDeliveryModalOpen] = useState(false);
  const [isThirdPartyModalOpen, setIsThirdPartyModalOpen] = useState(false);

  const isDeliveryEnabled = useModuleEnabled("delivery");

  const tableId = useBillingStore((state) => state.tableId);
  const setTableId = useBillingStore((state) => state.setTableId);
  const currentOrderType = useBillingStore((state) => state.orderType);
  const setOrderType = useBillingStore((state) => state.setOrderType);
  const deliveryDetails = useBillingStore((state) => state.deliveryDetails);
  const setDeliveryDetails = useBillingStore((state) => state.setDeliveryDetails);
  const setPaymentMethod = useBillingStore((state) => state.setPaymentMethod);

  const cart = useBillingStore((state) => state.cart);
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);
  const loadParkedCart = useBillingStore((state) => state.loadParkedCart);

  const reloadTables = async (): Promise<TableSummary[]> => {
    setIsLoading(true);
    try {
      const list = await getTables();
      setTables(list);
      return list;
    } catch (e) {
      setError((e as Error).message);
      return [];
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void reloadTables();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selectedTable = tables.find((t) => t.id === tableId);
  const effectiveOrderType = tableId !== null ? "dineIn" : currentOrderType;

  const saveToTable = async () => {
    if (tableId === null || cartOrder.length === 0) return;
    await runAction(setIsBusy, setError, async () => {
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
      void reloadTables();
      onParked(`Cart saved to ${tableName}.`);
    });
  };

  const resumeParkedOrder = async () => {
    if (tableId === null) return;
    await runAction(setIsBusy, setError, async () => {
      const [parked, items] = await Promise.all([getParkedCart(tableId), getItems({})]);
      if (!parked) return;
      const byId = new Map(items.map((item) => [item.id, item]));
      loadParkedCart(parked.items, parked.discountMinor, (id) => byId.get(id));
      setTableId(tableId);
    });
  };

  // "Print Token" needs a `table_orders` row to hang tokens off of — the
  // dialog reads pending items from `cart_json`, not the live billing cart.
  // So opening it first syncs whatever's in the cart right now to the table
  // (same write `saveToTable` does), but — unlike `saveToTable` — never
  // clears the cart: the cashier is continuing to work this table live, not
  // parking it away to bill later.
  const openTableTokenDialog = async () => {
    if (tableId === null) return;
    await runAction(setIsBusy, setError, async () => {
      if (cartOrder.length > 0) {
        const lines = cartOrder.map((id) => ({ itemId: id, qty: cart[id].qty }));
        const { discountMinor } = computeCartTotals(
          cartOrder.map((id) => cart[id]),
          discountMode,
          discountValue,
          taxPercent,
        );
        await attachCartToTable(tableId, lines, discountMinor);
      }
      const list = await reloadTables();
      const openOrderId = list.find((t) => t.id === tableId)?.openOrderId ?? null;
      if (openOrderId === null) {
        setError("Nothing to token yet — add items to the cart first.");
        return;
      }
      setTokenSource({ kind: "table", tableOrderId: openOrderId, tableName: selectedTable?.name ?? "the table" });
    });
  };

  // A Takeaway order has no table and so no `table_orders` row to sync to —
  // the dialog reads straight off the live cart instead (see `TokenPrintDialog`'s
  // "adhoc" source and `tokensService.getAdhocTokenGroups`'s doc comment for
  // the trade-off that implies: no delta tracking, every print is the full
  // cart's counter-eligible items).
  const openTakeawayTokenDialog = () => {
    if (cartOrder.length === 0) return;
    setTokenSource({ kind: "adhoc", items: cartOrder.map((id) => ({ itemId: id, qty: cart[id].qty })) });
  };

  // Ctrl/Cmd+K (see `useFastBillingHotkeys`) re-triggers whichever "Print
  // token" flow the mouse button for the current order type would call —
  // dine-in's `openTableTokenDialog` (with a table selected) or Takeaway's
  // `openTakeawayTokenDialog` (with `tableId === null`), same branch the
  // mouse buttons below are rendered on. `openTableTokenDialog` already
  // handles every "nothing to token" case with the same message the mouse
  // path shows; Takeaway has no such flow of its own (an empty cart is
  // simply nothing to token), so that case is handled here directly. Skipped
  // on mount: only a *change* in the request id counts as a keypress.
  const lastTokenPrintRequestId = useRef(tokenPrintRequestId);
  useEffect(() => {
    if (tokenPrintRequestId === lastTokenPrintRequestId.current) return;
    lastTokenPrintRequestId.current = tokenPrintRequestId;
    if (isBusy || tokenSource !== null) return;
    if (tableId !== null) {
      void openTableTokenDialog();
    } else if (cartOrder.length > 0) {
      openTakeawayTokenDialog();
    } else {
      setError("Nothing to token yet — add items to the cart first.");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tokenPrintRequestId]);

  useEffect(() => {
    onTokenDialogOpenChange(tokenSource !== null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tokenSource]);

  return (
    <div className="space-y-2">
      <div className="grid grid-cols-2 gap-2.5">
        {effectiveOrderType === "delivery" ? (
          <button
            type="button"
            onClick={() => setIsDeliveryModalOpen(true)}
            className="flex items-center justify-between gap-1.5 rounded-xl border border-line bg-surface-2 px-3 py-2 text-xs font-medium text-fg hover:border-accent transition text-left truncate"
            title="Edit Delivery Details"
          >
            <div className="flex items-center gap-1.5 truncate">
              <Bike className="h-3.5 w-3.5 text-accent-text shrink-0" />
              <span className="truncate">
                {deliveryDetails?.customerPhone
                  ? `${deliveryDetails.customerPhone} · ${deliveryDetails.deliveryRiderName || "Rider"}`
                  : "Set Delivery Info"}
              </span>
            </div>
            <Edit2 className="h-3 w-3 text-fg-muted shrink-0" />
          </button>
        ) : effectiveOrderType === "third_party_delivery" ? (
          <button
            type="button"
            onClick={() => setIsThirdPartyModalOpen(true)}
            className="flex items-center justify-between gap-1.5 rounded-xl border border-line bg-surface-2 px-3 py-2 text-xs font-medium text-fg hover:border-accent transition text-left truncate"
            title="Edit Platform Details"
          >
            <div className="flex items-center gap-1.5 truncate">
              <Store className="h-3.5 w-3.5 text-accent-text shrink-0" />
              <span className="truncate">
                {deliveryDetails?.deliveryPlatformName
                  ? `${deliveryDetails.deliveryPlatformName}${deliveryDetails.platformOrderRef ? ` (${deliveryDetails.platformOrderRef})` : ""}`
                  : "Select Platform"}
              </span>
            </div>
            <Edit2 className="h-3 w-3 text-fg-muted shrink-0" />
          </button>
        ) : (
          <DropdownSelect
            value={tableId}
            placeholder="Select Table"
            disabled={isLoading}
            onChange={(id) => {
              setTableId(id);
              if (id !== null) {
                setOrderType("dine_in");
                setDeliveryDetails(null);
              }
            }}
            options={tables.map((table) => ({
              value: table.id,
              label: table.name,
              meta: (
                <span className="flex items-center gap-1.5 text-xs font-medium text-fg-muted">
                  <span className={`h-1.5 w-1.5 rounded-full ${STATUS_DOT[table.status]}`} />
                  {STATUS_LABEL[table.status]}
                  {table.hasParkedOrder ? " · parked" : ""}
                </span>
              ),
            }))}
          />
        )}

        <DropdownSelect
          value={effectiveOrderType}
          placeholder="Order Type"
          onChange={(next) => {
            if (next === "dineIn") {
              setOrderType("dine_in");
              setDeliveryDetails(null);
            } else if (next === "takeaway") {
              setTableId(null);
              setOrderType("takeaway");
              setDeliveryDetails(null);
            } else if (next === "delivery") {
              setTableId(null);
              setOrderType("delivery");
              setIsDeliveryModalOpen(true);
            } else if (next === "third_party_delivery") {
              setTableId(null);
              setOrderType("third_party_delivery");
              setPaymentMethod("platform");
              setIsThirdPartyModalOpen(true);
            }
          }}
          options={[
            { value: "dineIn", label: ORDER_TYPE_LABELS.dine_in, disabled: tableId === null && tables.length === 0 },
            { value: "takeaway", label: ORDER_TYPE_LABELS.takeaway },
            ...(isDeliveryEnabled
              ? [
                  { value: "delivery", label: ORDER_TYPE_LABELS.delivery },
                  { value: "third_party_delivery", label: ORDER_TYPE_LABELS.third_party_delivery },
                ]
              : []),
          ]}
        />
      </div>

      {error && <p className="text-xs text-danger-text">{error}</p>}

      {selectedTable?.hasParkedOrder && (
        <button
          type="button"
          onClick={() => void resumeParkedOrder()}
          disabled={isBusy}
          className="w-full rounded-xl bg-warning-soft px-3 py-1.5 text-xs font-medium text-warning-text hover:bg-warning-soft disabled:opacity-50"
        >
          Load parked order from {selectedTable.name}
        </button>
      )}

      {tableId !== null && (cartOrder.length > 0 || selectedTable?.hasParkedOrder) && (
        <div className="grid grid-cols-2 gap-2">
          {cartOrder.length > 0 && (
            <button
              type="button"
              onClick={() => void saveToTable()}
              disabled={isBusy}
              className="rounded-xl border border-line px-3 py-1.5 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
            >
              Save to table
            </button>
          )}

          <button
            type="button"
            onClick={() => void openTableTokenDialog()}
            disabled={isBusy}
            className={`flex items-center justify-center gap-1.5 rounded-xl border border-line px-3 py-1.5 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50 ${cartOrder.length === 0 ? "col-span-2" : ""}`}
          >
            <Ticket className="h-3.5 w-3.5" />
            Print token
          </button>
        </div>
      )}

      {/* Same action for a Takeaway or Delivery order — no table, no parked order, so
          straight off the live cart (see `openTakeawayTokenDialog`). */}
      {tableId === null && cartOrder.length > 0 && (
        <button
          type="button"
          onClick={openTakeawayTokenDialog}
          disabled={isBusy}
          className="flex w-full items-center justify-center gap-1.5 rounded-xl border border-line px-3 py-1.5 text-xs font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
        >
          <Ticket className="h-3.5 w-3.5" />
          Print token
        </button>
      )}

      {tokenSource && (
        <TokenPrintDialog source={tokenSource} currency={currency} onClose={() => setTokenSource(null)} />
      )}

      {isDeliveryModalOpen && (
        <DeliveryDetailsModal
          onClose={() => setIsDeliveryModalOpen(false)}
          onSaved={(details) => {
            setDeliveryDetails(details);
            setIsDeliveryModalOpen(false);
          }}
        />
      )}

      {isThirdPartyModalOpen && (
        <ThirdPartyDeliveryModal
          onClose={() => setIsThirdPartyModalOpen(false)}
          onSaved={(details) => {
            setDeliveryDetails(details);
            setPaymentMethod("platform");
            setIsThirdPartyModalOpen(false);
          }}
        />
      )}
    </div>
  );
}
