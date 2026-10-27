import { useEffect, useState } from "react";
import { Calendar, Clock, Power, ReceiptText, Wallet } from "lucide-react";
import { useAppConfig } from "../../hooks/useAppConfig";
import { useModuleEnabled } from "../../hooks/useModules";
import { isAdminRole } from "../../utils/permissions";
import { useAuthStore, useBillingStore, useShiftStore } from "../../store";
import { CloseShiftModal } from "./CloseShiftModal";
import { OpenShiftModal } from "./OpenShiftModal";
import { RefundModal } from "./RefundModal";
import { CloudSyncBadge } from "../layout/CloudSyncBadge";

interface BillingHeaderProps {
  /** Called once a refund is created, so the item grid picks up the stock
   * `create_refund` just put back — the same "stock just changed, reload
   * the catalog" step `BillingPage.completeSale` already does after a sale. */
  onRefunded: () => void;
  onOpenActiveOrders?: () => void;
  activeOrdersCount?: number;
  hasLateOrders?: boolean;
}

/** Date/time + order-status strip above the item grid, plus the Open/Close
 * Shift affordance when the `shifts` module is enabled. Order status is
 * derived purely from whether the cart has anything in it — never a
 * hardcoded "always open" flag — so it actually reflects the till's state. */
export function BillingHeader({
  onRefunded,
  onOpenActiveOrders,
  activeOrdersCount = 0,
  hasLateOrders = false,
}: BillingHeaderProps) {
  const [now, setNow] = useState(() => new Date());
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const clearCart = useBillingStore((state) => state.clearCart);
  const { config } = useAppConfig();
  const shiftsEnabled = useModuleEnabled("shifts");
  const openShift = useShiftStore((state) => state.openShift);
  const role = useAuthStore((state) => state.user?.role);
  const canRefund = role ? isAdminRole(role) : false;

  const [showOpenModal, setShowOpenModal] = useState(false);
  const [showCloseModal, setShowCloseModal] = useState(false);
  const [showRefundModal, setShowRefundModal] = useState(false);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  const hasItems = cartOrder.length > 0;
  const dateLabel = now.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short", year: "numeric" });
  const timeLabel = now.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

  return (
    <div className="billing-header flex flex-wrap items-center justify-between gap-2">
      <div className="flex min-w-0 flex-wrap items-center gap-3">
        <div>
          <h2 className="text-xl sm:text-2xl font-bold tracking-tight text-fg">Billing</h2>
          <p className="hidden md:block text-xs text-fg-muted">Find an item, build an order, and check out.</p>
        </div>
        <div className="hidden xl:flex items-center gap-1.5 rounded-xl border border-line bg-surface px-2.5 py-1 text-xs font-medium text-fg-2 shadow-soft">
          <Calendar className="h-3 w-3 text-fg-muted" />
          <span>{dateLabel}</span>
        </div>
        <div className="hidden lg:flex items-center gap-1.5 rounded-xl border border-line bg-surface px-2.5 py-1 text-xs font-medium text-fg-2 shadow-soft">
          <Clock className="h-3 w-3 text-fg-muted" />
          <span>{timeLabel}</span>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-1.5 sm:gap-2">
        {onOpenActiveOrders && (
          <button
            type="button"
            onClick={onOpenActiveOrders}
            className={`flex items-center gap-1.5 rounded-xl border px-2.5 sm:px-3 py-1.5 text-xs font-semibold shadow-soft transition ${
              hasLateOrders
                ? "border-danger bg-danger-soft text-danger-text animate-pulse"
                : activeOrdersCount > 0
                  ? "border-accent bg-accent-soft text-accent-text hover:bg-accent-soft/80"
                  : "border-line bg-surface text-fg-muted hover:bg-surface-2"
            }`}
          >
            <Clock className="h-3.5 w-3.5" />
            <span>Active Orders</span>
            {activeOrdersCount > 0 && (
              <span
                className={`ml-0.5 rounded-full px-1.5 py-0.2 text-[10px] font-bold ${
                  hasLateOrders ? "bg-danger text-white" : "bg-accent text-accent-ink"
                }`}
              >
                {activeOrdersCount}
              </span>
            )}
          </button>
        )}

        <CloudSyncBadge />

        {canRefund && (
          <button
            type="button"
            onClick={() => setShowRefundModal(true)}
            className="flex items-center gap-1.5 rounded-xl border border-line bg-surface px-2.5 sm:px-3 py-1.5 text-xs font-semibold text-fg-muted shadow-soft hover:bg-surface-2"
          >
            <ReceiptText className="h-3.5 w-3.5" />
            <span>Refund</span>
          </button>
        )}

        {shiftsEnabled &&
          (openShift ? (
            <button
              type="button"
              onClick={() => setShowCloseModal(true)}
              className="flex items-center gap-1.5 rounded-xl border border-line bg-surface px-2.5 sm:px-3 py-1.5 text-xs font-semibold text-accent-text shadow-soft hover:bg-accent-soft"
            >
              <Wallet className="h-3.5 w-3.5" />
              <span>Shift #{openShift.id}</span>
            </button>
          ) : (
            <button
              type="button"
              onClick={() => setShowOpenModal(true)}
              className="flex items-center gap-1.5 rounded-xl border border-line bg-surface px-2.5 sm:px-3 py-1.5 text-xs font-semibold text-fg-muted shadow-soft hover:bg-surface-2"
            >
              <Wallet className="h-3.5 w-3.5" />
              <span>Open Shift</span>
            </button>
          ))}

        <div className="flex items-center gap-1.5 rounded-xl border border-line bg-surface py-1 pl-2.5 sm:pl-3 pr-1 shadow-soft">
          <span className={`h-2 w-2 rounded-full ${hasItems ? "bg-success" : "bg-fg-faint"}`} />
          <span className={`text-xs font-semibold ${hasItems ? "text-success-text" : "text-fg-muted"}`}>
            {hasItems ? "In progress" : "Idle"}
          </span>
          <button
            type="button"
            onClick={() => hasItems && clearCart()}
            disabled={!hasItems}
            aria-label="Clear the current order"
            title={hasItems ? "Clear the current order" : undefined}
            className="flex h-6 w-6 items-center justify-center rounded-full text-fg-muted hover:bg-surface-3 hover:text-fg-2 disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent"
          >
            <Power className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {showOpenModal && <OpenShiftModal currency={config.currency} onClose={() => setShowOpenModal(false)} onOpened={() => setShowOpenModal(false)} />}
      {showCloseModal && openShift && (
        <CloseShiftModal shiftId={openShift.id} config={config} onClose={() => setShowCloseModal(false)} />
      )}
      {showRefundModal && (
        <RefundModal
          config={config}
          onClose={() => setShowRefundModal(false)}
          onRefunded={onRefunded}
        />
      )}
    </div>
  );
}
