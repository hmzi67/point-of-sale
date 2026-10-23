import { useState } from "react";
import { ClipboardList, Pencil, UserRound, X } from "lucide-react";
import { useModuleEnabled } from "../../hooks/useModules";
import { useBillingStore } from "../../store";
import { formatMinor } from "../../utils/format";
import { CustomerPickerModal } from "./CustomerPickerModal";

/** Customer + a local draft order number. With the `customers` module on,
 * the pencil attaches a real customer account (needed for pay later);
 * otherwise the name is just a label on this draft order. Either way it's
 * cleared the moment the order is completed or reset. */
export function OrderSummaryHeader({ currency }: { currency: string }) {
  const customersEnabled = useModuleEnabled("customers");
  const customerName = useBillingStore((state) => state.customerName);
  const setCustomerName = useBillingStore((state) => state.setCustomerName);
  const customer = useBillingStore((state) => state.customer);
  const setCustomer = useBillingStore((state) => state.setCustomer);
  const draftOrderNumber = useBillingStore((state) => state.draftOrderNumber);

  const [isEditing, setIsEditing] = useState(false);
  const [isPicking, setIsPicking] = useState(false);
  const [draft, setDraft] = useState(customerName);

  const startEditing = () => {
    if (customersEnabled) {
      setIsPicking(true);
      return;
    }
    setDraft(customerName);
    setIsEditing(true);
  };

  const commit = () => {
    setCustomerName(draft.trim());
    setIsEditing(false);
  };

  return (
    <div className="flex items-center justify-between gap-3 border-b border-line pb-2.5 sm:pb-3">
      <div className="flex min-w-0 items-center gap-2.5 sm:gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-accent-soft text-accent-text sm:h-10 sm:w-10">
          {customer ? <UserRound className="h-4 w-4 sm:h-5 sm:w-5" /> : <ClipboardList className="h-4 w-4 sm:h-5 sm:w-5" />}
        </span>
        <div className="min-w-0">
          {isEditing ? (
            <input
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onBlur={commit}
              onKeyDown={(e) => {
                if (e.key === "Enter") commit();
                if (e.key === "Escape") setIsEditing(false);
              }}
              placeholder="Counter Sale"
              className="w-36 sm:w-40 border-b border-accent/40 bg-transparent text-sm font-semibold text-fg focus:outline-none"
            />
          ) : (
            <p className="truncate text-sm font-semibold text-fg sm:text-base">{customerName || "Counter Sale"}</p>
          )}
          <p className="truncate text-[11px] font-medium text-fg-muted">
            Order #{String(draftOrderNumber).padStart(3, "0")}
            {customer?.phone ? ` · ${customer.phone}` : ""}
            {customer && customer.balanceMinor > 0 && (
              <span className={customer.overdueMinor > 0 ? "text-danger-text" : "text-warning-text"}>
                {` · owes ${formatMinor(customer.balanceMinor, currency)}`}
              </span>
            )}
          </p>
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1.5">
        {customer && (
          <button
            type="button"
            onClick={() => setCustomer(null)}
            aria-label="Remove customer"
            className="rounded-xl border border-line p-1.5 text-fg-muted hover:bg-surface-3 hover:text-fg-2"
          >
            <X className="h-3.5 w-3.5" />
          </button>
        )}
        <button
          type="button"
          onClick={startEditing}
          aria-label={customersEnabled ? "Choose customer" : "Edit customer name"}
          className="rounded-xl border border-line p-1.5 text-fg-muted hover:bg-surface-3 hover:text-fg-2"
        >
          {customersEnabled ? <UserRound className="h-3.5 w-3.5" /> : <Pencil className="h-3.5 w-3.5" />}
        </button>
      </div>

      {isPicking && (
        <CustomerPickerModal
          currency={currency}
          onPick={(picked) => {
            setCustomer(picked);
            setIsPicking(false);
          }}
          onClose={() => setIsPicking(false)}
        />
      )}
    </div>
  );
}
