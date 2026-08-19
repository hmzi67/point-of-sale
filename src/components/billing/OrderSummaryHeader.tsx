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
    <div className="flex items-center justify-between gap-3">
      <div className="flex items-center gap-3">
        <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-brand-50 text-brand-600">
          <ClipboardList className="h-4 w-4" />
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
              className="w-40 border-b border-brand-300 bg-transparent text-sm font-semibold text-slate-900 focus:outline-none"
            />
          ) : (
            <p className="text-sm font-semibold text-slate-900">{customerName || "Counter Sale"}</p>
          )}
          <p className="text-xs text-slate-400">Order Number #{String(draftOrderNumber).padStart(3, "0")}</p>
        </div>
      </div>

      <button
        type="button"
        onClick={startEditing}
        aria-label="Edit customer name"
        className="rounded-full p-2 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
      >
        <Pencil className="h-4 w-4" />
      </button>
    </div>
  );
}
