import { useState } from "react";
import { ClipboardList, Pencil } from "lucide-react";
import { useBillingStore } from "../../store";

/** Customer name (editable — just a label on this draft order, not a real
 * customer record) + a local draft order number. Neither is sent anywhere
 * or persisted; they're both cleared the moment the order is completed or
 * reset, same as the rest of the in-progress cart. */
export function OrderSummaryHeader() {
  const customerName = useBillingStore((state) => state.customerName);
  const setCustomerName = useBillingStore((state) => state.setCustomerName);
  const draftOrderNumber = useBillingStore((state) => state.draftOrderNumber);

  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(customerName);

  const startEditing = () => {
    setDraft(customerName);
    setIsEditing(true);
  };

  const commit = () => {
    setCustomerName(draft.trim());
    setIsEditing(false);
  };

  return (
    <div className="flex items-center justify-between gap-3 border-b border-line pb-2.5 sm:pb-3">
      <div className="flex items-center gap-2.5 sm:gap-3">
        <span className="flex h-9 w-9 sm:h-10 sm:w-10 items-center justify-center rounded-xl bg-accent-soft text-accent-text">
          <ClipboardList className="h-4 w-4 sm:h-5 sm:w-5" />
        </span>
        <div>
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
            <p className="text-sm sm:text-base font-semibold text-fg">{customerName || "Counter Sale"}</p>
          )}
          <p className="text-[11px] font-medium text-fg-muted">Order #{String(draftOrderNumber).padStart(3, "0")}</p>
        </div>
      </div>

      <button
        type="button"
        onClick={startEditing}
        aria-label="Edit customer name"
        className="rounded-xl border border-line p-1.5 text-fg-muted hover:bg-surface-3 hover:text-fg-2"
      >
        <Pencil className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}
