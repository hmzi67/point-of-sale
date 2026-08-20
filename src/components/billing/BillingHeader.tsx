import { useEffect, useState } from "react";
import { Calendar, Clock, Power, ReceiptText, Wallet } from "lucide-react";
import { useAppConfig } from "../../hooks/useAppConfig";
import { useModules } from "../../hooks/useModules";
import { isAdminRole } from "../../utils/permissions";
import { useAuthStore, useBillingStore, useShiftStore } from "../../store";
import { CloseShiftModal } from "./CloseShiftModal";
import { OpenShiftModal } from "./OpenShiftModal";
import { RefundModal } from "./RefundModal";

interface BillingHeaderProps {
  /** Called once a refund is created, so the item grid picks up the stock
   * `create_refund` just put back — the same "stock just changed, reload
   * the catalog" step `BillingPage.completeSale` already does after a sale. */
  onRefunded: () => void;
}

/** Date/time + order-status strip above the item grid, plus the Open/Close
 * Shift affordance when the `shifts` module is enabled. Order status is
 * derived purely from whether the cart has anything in it — never a
 * hardcoded "always open" flag — so it actually reflects the till's state. */
export function BillingHeader({ onRefunded }: BillingHeaderProps) {
  const [now, setNow] = useState(() => new Date());
  const cartOrder = useBillingStore((state) => state.cartOrder);
  const clearCart = useBillingStore((state) => state.clearCart);
  const { modules } = useModules();
  const { config } = useAppConfig();
  const shiftsEnabled = modules.some((m) => m.key === "shifts" && m.enabled);
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
    <div className="flex flex-wrap items-center justify-between gap-2">
      <div className="flex items-center gap-2">
        <span className="flex items-center gap-1.5 rounded-full bg-white px-3.5 py-2 text-xs font-medium text-slate-600 shadow-soft">
          <Calendar className="h-3.5 w-3.5 text-slate-400" />
          {dateLabel}
        </span>
        <span className="flex items-center gap-1.5 rounded-full bg-white px-3.5 py-2 text-xs font-medium text-slate-600 shadow-soft">
          <Clock className="h-3.5 w-3.5 text-slate-400" />
          {timeLabel}
        </span>
      </div>

      <div className="flex items-center gap-2">
        {canRefund && (
          <button
            type="button"
            onClick={() => setShowRefundModal(true)}
            className="flex items-center gap-1.5 rounded-full bg-white px-3.5 py-2 text-xs font-semibold text-slate-500 shadow-soft hover:bg-slate-50"
          >
            <ReceiptText className="h-3.5 w-3.5" />
            Refund
          </button>
        )}

        {shiftsEnabled &&
          (openShift ? (
            <button
              type="button"
              onClick={() => setShowCloseModal(true)}
              className="flex items-center gap-1.5 rounded-full bg-white px-3.5 py-2 text-xs font-semibold text-brand-700 shadow-soft hover:bg-brand-50"
            >
              <Wallet className="h-3.5 w-3.5" />
              Close Shift #{openShift.id}
            </button>
          ) : (
            <button
              type="button"
              onClick={() => setShowOpenModal(true)}
              className="flex items-center gap-1.5 rounded-full bg-white px-3.5 py-2 text-xs font-semibold text-slate-500 shadow-soft hover:bg-slate-50"
            >
              <Wallet className="h-3.5 w-3.5" />
              Open Shift
            </button>
          ))}

        <div className="flex items-center gap-1.5 rounded-full bg-white py-1.5 pl-3.5 pr-1.5 shadow-soft">
          <span className={`h-2 w-2 rounded-full ${hasItems ? "bg-emerald-500" : "bg-red-500"}`} />
          <span className={`text-xs font-semibold ${hasItems ? "text-emerald-600" : "text-red-500"}`}>
            {hasItems ? "Open Order" : "Close Order"}
          </span>
          <button
            type="button"
            onClick={() => hasItems && clearCart()}
            disabled={!hasItems}
            title={hasItems ? "Clear the current order" : undefined}
            className="flex h-6 w-6 items-center justify-center rounded-full text-slate-400 hover:bg-slate-100 hover:text-slate-600 disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent"
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
