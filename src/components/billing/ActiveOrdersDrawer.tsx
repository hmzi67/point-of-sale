import { useEffect, useState } from "react";
import { AlertCircle, Bike, CheckCircle2, ChevronRight, Clock, Coffee, Printer, RefreshCw, ShoppingBag, Store, Trash2, Utensils, X } from "lucide-react";
import { getItems } from "../../services/inventoryService";
import {
  cancelOrder,
  listActiveOrders,
  printOrderToken,
  resumeOrder,
  transitionOrder,
  updateDeliveryStatus,
  updateOrderPrepTime,
} from "../../services/ordersService";
import { useBillingStore } from "../../store";
import type { DeliveryStatus, Item, Order, OrderStatus } from "../../types";
import { formatMinor } from "../../utils/format";
import { DEFAULT_PREP_MINUTES, isDeliveryOrder, ORDER_TYPE_LABELS, THIRD_PARTY_FALLBACK_LABEL } from "../../utils/orderType";
import { runAction } from "../../utils/runAction";

interface ActiveOrdersDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  currency: string;
  onOrderResumed: (order: Order) => void;
}

export function ActiveOrdersDrawer({
  isOpen,
  onClose,
  currency,
  onOrderResumed,
}: ActiveOrdersDrawerProps) {
  const [orders, setOrders] = useState<Order[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [filterType, setFilterType] = useState<
    "all" | "takeaway" | "dine_in" | "delivery" | "third_party_delivery"
  >("all");
  const [now, setNow] = useState(Date.now());
  const [busyOrderId, setBusyOrderId] = useState<number | null>(null);

  const loadParkedCart = useBillingStore((state) => state.loadParkedCart);
  const setActiveOrder = useBillingStore((state) => state.setActiveOrder);
  const setTableId = useBillingStore((state) => state.setTableId);
  const setCustomerName = useBillingStore((state) => state.setCustomerName);

  const fetchOrders = async () => {
    await runAction(setIsLoading, setError, async () => {
      const active = await listActiveOrders();
      setOrders(active);
    });
  };

  useEffect(() => {
    if (isOpen) {
      void fetchOrders();
    }
  }, [isOpen]);

  // Tick elapsed time every 10 seconds while open
  useEffect(() => {
    if (!isOpen) return;
    const interval = window.setInterval(() => {
      setNow(Date.now());
    }, 10000);
    return () => window.clearInterval(interval);
  }, [isOpen]);

  if (!isOpen) return null;

  /** `runAction` with this drawer's per-order busy flag. */
  const runForOrder = (orderId: number, action: () => Promise<unknown>) =>
    runAction((busy) => setBusyOrderId(busy ? orderId : null), setError, action);

  const handleResume = (order: Order) =>
    runForOrder(order.id, async () => {
      // 1. Fetch live catalog to populate items correctly
      const catalog = await getItems({});
      const byId = new Map<number, Item>(catalog.map((i) => [i.id, i]));

      // 2. If it was held, resume status in the DB
      let updated = order;
      if (order.status === "held") {
        updated = await resumeOrder(order.id, order.version);
      }

      // 3. Load active unpaid lines into cart
      const activeLines = updated.lines
        .filter((l) => l.status === "active" && l.saleId === null)
        .map((l) => ({ itemId: l.itemId, qty: l.qty }));

      loadParkedCart(activeLines, 0, (id) => byId.get(id));
      setActiveOrder(updated.id, updated.version);
      setTableId(updated.tableId);
      setCustomerName(updated.customerName || "");

      useBillingStore.getState().setOrderType(updated.orderType);
      if (isDeliveryOrder(updated.orderType)) {
        useBillingStore.getState().setDeliveryDetails({
          deliveryRiderId: updated.deliveryRiderId,
          riderSource: updated.riderSource,
          deliveryRiderName: updated.deliveryRiderName,
          deliveryRiderPhone: updated.deliveryRiderPhone,
          deliveryPlatformId: updated.deliveryPlatformId,
          deliveryPlatformName: updated.deliveryPlatformName,
          customerPhone: updated.customerPhone,
          deliveryAddress: updated.deliveryAddress,
          deliveryChargeMinor: updated.deliveryChargeMinor,
          deliveryStatus: updated.deliveryStatus,
          platformOrderRef: updated.platformOrderRef,
        });
      } else {
        useBillingStore.getState().setDeliveryDetails(null);
      }

      onOrderResumed(updated);
      onClose();
    });

  const handlePrintToken = (orderId: number) => runForOrder(orderId, () => printOrderToken(orderId, false));

  const handleTransition = (order: Order, toStatus: OrderStatus) =>
    runForOrder(order.id, async () => {
      await transitionOrder(order.id, order.version, toStatus);
      await fetchOrders();
    });

  const handleExtendPrepTime = (order: Order, addedMinutes: number) =>
    runForOrder(order.id, async () => {
      await updateOrderPrepTime(order.id, (order.estimatedPrepMinutes ?? DEFAULT_PREP_MINUTES) + addedMinutes);
      await fetchOrders();
    });

  const handleCancel = (order: Order) => {
    if (!window.confirm(`Are you sure you want to cancel Order #${order.orderNo}?`)) return;
    return runForOrder(order.id, async () => {
      await cancelOrder(order.id, order.version, "Cancelled from active orders drawer");
      await fetchOrders();
    });
  };

  const handleDeliveryStatus = (orderId: number, status: DeliveryStatus) =>
    runForOrder(orderId, async () => {
      await updateDeliveryStatus(orderId, status);
      await fetchOrders();
    });

  const filteredOrders = orders.filter((o) => {
    if (filterType === "all") return true;
    return o.orderType === filterType;
  });

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-40 bg-scrim transition-opacity"
        onClick={onClose}
      />

      {/* Drawer */}
      <div className="fixed inset-y-0 right-0 z-50 flex w-full max-w-lg flex-col border-l border-line bg-surface shadow-soft-xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-line px-5 py-4">
          <div className="flex items-center gap-2.5">
            <div className="rounded-xl bg-accent-soft p-2 text-accent-text">
              <Clock className="h-5 w-5" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="text-base font-semibold text-fg">Active Orders</h2>
                <span className="rounded-full bg-surface-2 px-2 py-0.5 text-xs font-semibold text-fg-muted">
                  {orders.length}
                </span>
              </div>
              <p className="text-xs text-fg-muted">Live preparation & pickup queue</p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={fetchOrders}
              disabled={isLoading}
              title="Refresh orders"
              className="rounded-lg p-1.5 text-fg-muted hover:bg-surface-2 disabled:opacity-50"
            >
              <RefreshCw className={`h-4 w-4 ${isLoading ? "animate-spin" : ""}`} />
            </button>
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg p-1.5 text-fg-muted hover:bg-surface-2"
              aria-label="Close"
            >
              <X className="h-5 w-5" />
            </button>
          </div>
        </div>

        {/* Filter Pills */}
        <div className="flex items-center gap-2 overflow-x-auto border-b border-line bg-surface-2/40 px-4 sm:px-5 py-2 sm:py-2.5 scrollbar-none [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          <button
            type="button"
            onClick={() => setFilterType("all")}
            className={`rounded-lg px-3 py-1.5 text-xs font-medium transition ${
              filterType === "all"
                ? "bg-accent text-accent-ink"
                : "text-fg-muted hover:bg-surface-2"
            }`}
          >
            All ({orders.length})
          </button>
          <button
            type="button"
            onClick={() => setFilterType("takeaway")}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition ${
              filterType === "takeaway"
                ? "bg-accent text-accent-ink"
                : "text-fg-muted hover:bg-surface-2"
            }`}
          >
            <ShoppingBag className="h-3.5 w-3.5" />
            Takeaway ({orders.filter((o) => o.orderType === "takeaway").length})
          </button>
          <button
            type="button"
            onClick={() => setFilterType("dine_in")}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition ${
              filterType === "dine_in"
                ? "bg-accent text-accent-ink"
                : "text-fg-muted hover:bg-surface-2"
            }`}
          >
            <Utensils className="h-3.5 w-3.5" />
            Dine-In ({orders.filter((o) => o.orderType === "dine_in").length})
          </button>
          <button
            type="button"
            onClick={() => setFilterType("delivery")}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition ${
              filterType === "delivery"
                ? "bg-accent text-accent-ink"
                : "text-fg-muted hover:bg-surface-2"
            }`}
          >
            <Bike className="h-3.5 w-3.5" />
            Delivery ({orders.filter((o) => o.orderType === "delivery").length})
          </button>
          <button
            type="button"
            onClick={() => setFilterType("third_party_delivery")}
            className={`flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition ${
              filterType === "third_party_delivery"
                ? "bg-accent text-accent-ink"
                : "text-fg-muted hover:bg-surface-2"
            }`}
          >
            <Store className="h-3.5 w-3.5" />
            3rd-Party ({orders.filter((o) => o.orderType === "third_party_delivery").length})
          </button>
        </div>

        {/* Error message */}
        {error && (
          <div className="m-4 rounded-xl bg-danger-soft p-3 text-xs text-danger-text">
            {error}
          </div>
        )}

        {/* List of active orders */}
        <div className="flex-1 space-y-3 overflow-y-auto p-5">
          {filteredOrders.length === 0 ? (
            <div className="flex h-64 flex-col items-center justify-center gap-2 text-center text-fg-muted">
              <Coffee className="h-8 w-8 stroke-1 text-fg-faint" />
              <p className="text-sm font-medium text-fg">No active orders</p>
              <p className="text-xs">
                Hold a cart or send a takeaway order to track it live here.
              </p>
            </div>
          ) : (
            filteredOrders.map((order) => {
              const createdTimestamp = new Date(order.createdAt).getTime();
              const elapsedMinutes = Math.max(
                0,
                Math.floor((now - (isNaN(createdTimestamp) ? now : createdTimestamp)) / 60000),
              );
              const targetMinutes = order.estimatedPrepMinutes ?? DEFAULT_PREP_MINUTES;
              const remainingMinutes = targetMinutes - elapsedMinutes;

              const isLate = remainingMinutes < 0;
              const isDueSoon = remainingMinutes <= 3 && !isLate;
              const isBusy = busyOrderId === order.id;

              return (
                <div
                  key={order.id}
                  className={`rounded-2xl border transition ${
                    isLate
                      ? "border-danger/60 bg-danger-soft/20 shadow-soft"
                      : isDueSoon
                        ? "border-warning/60 bg-warning-soft/20 shadow-soft"
                        : "border-line bg-surface hover:border-line-strong"
                  } p-4`}
                >
                  {/* Card top row */}
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-base font-bold text-fg">
                          #{order.orderNo}
                        </span>
                        <span
                          className={`inline-flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wider ${
                            order.orderType === "takeaway"
                              ? "bg-accent-soft text-accent-text"
                              : order.orderType === "delivery"
                                ? "bg-info-soft text-info-text"
                                : order.orderType === "third_party_delivery"
                                  ? "bg-accent-soft text-accent-text"
                                  : "bg-surface-2 text-fg-muted"
                          }`}
                        >
                          {order.orderType === "delivery" ? (
                            <>
                              <Bike className="h-3 w-3" /> {ORDER_TYPE_LABELS.delivery}
                            </>
                          ) : order.orderType === "third_party_delivery" ? (
                            <>
                              <Store className="h-3 w-3" /> {order.deliveryPlatformName || THIRD_PARTY_FALLBACK_LABEL}
                            </>
                          ) : order.orderType === "takeaway" ? (
                            ORDER_TYPE_LABELS.takeaway
                          ) : (
                            order.tableName ?? ORDER_TYPE_LABELS.dine_in
                          )}
                        </span>
                        {order.status === "ready" && (
                          <span className="flex items-center gap-1 rounded-md bg-success-soft px-2 py-0.5 text-[11px] font-semibold text-success-text">
                            <CheckCircle2 className="h-3 w-3" /> Ready
                          </span>
                        )}
                        {isDeliveryOrder(order.orderType) && order.deliveryStatus && (
                          <span
                            className={`rounded-md px-2 py-0.5 text-[11px] font-semibold capitalize ${
                              order.deliveryStatus === "delivered"
                                ? "bg-success-soft text-success-text"
                                : order.deliveryStatus === "dispatched"
                                  ? "bg-warning-soft text-warning-text"
                                  : "bg-surface-2 text-fg-muted"
                            }`}
                          >
                            {order.deliveryStatus}
                          </span>
                        )}
                      </div>

                      {order.customerName && (
                        <p className="mt-0.5 text-xs font-medium text-fg">
                          Customer: {order.customerName}
                          {order.customerPhone ? ` (${order.customerPhone})` : ""}
                        </p>
                      )}

                      {order.orderType === "delivery" && (
                        <div className="mt-1 space-y-0.5 text-xs text-fg-muted">
                          {order.deliveryAddress && (
                            <p className="line-clamp-2">
                              <span className="font-medium text-fg">Address:</span> {order.deliveryAddress}
                            </p>
                          )}
                          {order.deliveryRiderName && (
                            <p>
                              <span className="font-medium text-fg">Rider:</span> {order.deliveryRiderName}
                              {order.deliveryRiderPhone ? ` (${order.deliveryRiderPhone})` : ""}
                            </p>
                          )}
                        </div>
                      )}

                      {order.orderType === "third_party_delivery" && (
                        <div className="mt-1 space-y-0.5 text-xs text-fg-muted">
                          {order.platformOrderRef && (
                            <p>
                              <span className="font-medium text-fg">Ref:</span> #{order.platformOrderRef}
                            </p>
                          )}
                          {order.deliveryRiderName && (
                            <p>
                              <span className="font-medium text-fg">Rider:</span> {order.deliveryRiderName}
                              {order.deliveryRiderPhone ? ` (${order.deliveryRiderPhone})` : ""}
                            </p>
                          )}
                        </div>
                      )}
                    </div>

                    {/* Payment tag */}
                    <div>
                      {order.paymentStatus === "paid" ? (
                        <span className="rounded-lg bg-success-soft px-2.5 py-1 text-xs font-semibold text-success-text">
                          PAID
                        </span>
                      ) : (
                        <span className="rounded-lg bg-surface-2 px-2.5 py-1 text-xs font-semibold text-fg">
                          Due: {formatMinor(order.dueMinor, currency)}
                        </span>
                      )}
                    </div>
                  </div>

                  {/* Timer & Target status */}
                  <div className="mt-3 flex items-center justify-between rounded-xl bg-surface-2/60 px-3 py-2 text-xs">
                    <div className="flex items-center gap-2">
                      <Clock
                        className={`h-4 w-4 ${
                          isLate
                            ? "text-danger-text animate-pulse"
                            : isDueSoon
                              ? "text-warning-text"
                              : "text-fg-muted"
                        }`}
                      />
                      <span className="font-medium text-fg">
                        {elapsedMinutes}m elapsed
                      </span>
                      <span className="text-fg-faint">/</span>
                      <span className="text-fg-muted">Target: {targetMinutes}m</span>
                    </div>

                    <div className="flex items-center gap-1.5">
                      {isLate ? (
                        <span className="flex items-center gap-1 font-semibold text-danger-text">
                          <AlertCircle className="h-3.5 w-3.5" />
                          LATE by {Math.abs(remainingMinutes)}m
                        </span>
                      ) : isDueSoon ? (
                        <span className="font-semibold text-warning-text">
                          Due in {remainingMinutes}m
                        </span>
                      ) : (
                        <span className="font-medium text-success-text">
                          {remainingMinutes}m left
                        </span>
                      )}

                      {/* Quick +5m delay button */}
                      <button
                        type="button"
                        onClick={() => handleExtendPrepTime(order, 5)}
                        disabled={isBusy}
                        title="Add 5 minutes prep time"
                        className="rounded bg-surface-3 px-1.5 py-0.5 text-[10px] font-semibold text-fg hover:bg-surface-2"
                      >
                        +5m
                      </button>
                    </div>
                  </div>

                  {/* Order items preview */}
                  <div className="mt-2.5 text-xs text-fg-muted">
                    <ul className="space-y-1">
                      {order.lines
                        .filter((l) => l.status === "active")
                        .slice(0, 4)
                        .map((line) => (
                          <li key={line.id} className="flex justify-between">
                            <span>
                              {line.qty}x {line.itemName}
                              {line.notes && (
                                <span className="italic text-fg-faint"> ({line.notes})</span>
                              )}
                            </span>
                            <span>{formatMinor(line.priceAtOrderMinor * line.qty, currency)}</span>
                          </li>
                        ))}
                      {order.lines.filter((l) => l.status === "active").length > 4 && (
                        <li className="text-[11px] text-fg-faint">
                          + {order.lines.filter((l) => l.status === "active").length - 4} more items...
                        </li>
                      )}
                    </ul>
                  </div>

                  {/* Actions footer */}
                  <div className="mt-3 flex items-center justify-between border-t border-line/60 pt-3">
                    <div className="flex items-center gap-1">
                      <button
                        type="button"
                        onClick={() => handleCancel(order)}
                        disabled={isBusy}
                        title="Cancel order"
                        className="rounded-lg p-1.5 text-danger-text hover:bg-danger-soft disabled:opacity-50"
                      >
                        <Trash2 className="h-4 w-4" />
                      </button>
                      <button
                        type="button"
                        onClick={() => handlePrintToken(order.id)}
                        disabled={isBusy}
                        title="Print / Reprint Token"
                        className="rounded-lg p-1.5 text-fg-muted hover:bg-surface-2 hover:text-fg disabled:opacity-50"
                      >
                        <Printer className="h-4 w-4" />
                      </button>
                    </div>

                    <div className="flex items-center gap-2">
                      {/* If unpaid, allow resuming directly into billing cart */}
                      {order.paymentStatus !== "paid" && (
                        <button
                          type="button"
                          onClick={() => handleResume(order)}
                          disabled={isBusy}
                          className="flex items-center gap-1.5 rounded-xl bg-surface-2 px-3.5 py-1.5 text-xs font-semibold text-fg hover:bg-surface-3 disabled:opacity-50"
                        >
                          Resume / Bill
                          <ChevronRight className="h-3.5 w-3.5" />
                        </button>
                      )}

                      {/* Delivery status actions */}
                      {isDeliveryOrder(order.orderType) && (
                        <>
                          {order.deliveryStatus === "placed" && (
                            <button
                              type="button"
                              onClick={() => handleDeliveryStatus(order.id, "dispatched")}
                              disabled={isBusy}
                              className="rounded-xl border border-warning/40 bg-warning-soft/20 px-3 py-1.5 text-xs font-semibold text-warning-text hover:bg-warning-soft/40 disabled:opacity-50"
                            >
                              Mark Dispatched
                            </button>
                          )}
                          {order.deliveryStatus === "dispatched" && (
                            <button
                              type="button"
                              onClick={() => handleDeliveryStatus(order.id, "delivered")}
                              disabled={isBusy}
                              className="rounded-xl border border-success/40 bg-success-soft/20 px-3 py-1.5 text-xs font-semibold text-success-text hover:bg-success-soft/40 disabled:opacity-50"
                            >
                              Mark Delivered
                            </button>
                          )}
                        </>
                      )}

                      {/* If order is not ready yet and unpaid, allow marking it ready */}
                      {order.status !== "ready" && order.status !== "served" && order.paymentStatus !== "paid" && (
                        <button
                          type="button"
                          onClick={() => handleTransition(order, "ready")}
                          disabled={isBusy}
                          className="rounded-xl border border-line bg-surface px-3 py-1.5 text-xs font-semibold text-fg hover:bg-surface-2 disabled:opacity-50"
                        >
                          Mark Ready
                        </button>
                      )}

                      {/* If ready or already paid, allow marking as handed over / served */}
                      {(order.status === "ready" || order.paymentStatus === "paid") && (
                        <button
                          type="button"
                          onClick={() => handleTransition(order, "served")}
                          disabled={isBusy}
                          className="rounded-xl bg-accent px-3.5 py-1.5 text-xs font-semibold text-accent-ink hover:opacity-90 disabled:opacity-50"
                        >
                          {order.paymentStatus === "paid" ? "Complete / Hand Over" : "Hand Over"}
                        </button>
                      )}
                    </div>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>
    </>
  );
}
