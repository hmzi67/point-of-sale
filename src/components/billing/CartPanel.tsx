import { ClipboardList } from "lucide-react";
import { useBillingStore } from "../../store";
import { CartRow } from "./CartRow";

interface CartPanelProps {
  currency: string;
  onEditLine: (itemId: number) => void;
}

/** Maps over the stable `cartOrder` id list — this only changes on add/remove,
 * so an in-place quantity edit never re-renders the list itself, only the one
 * `CartRow` whose entry changed. */
export function CartPanel({ currency, onEditLine }: CartPanelProps) {
  const cartOrder = useBillingStore((state) => state.cartOrder);

  if (cartOrder.length === 0) {
    // flex-1 here is exactly right (see the non-empty branch's comment) —
    // an empty cart still occupies the same "middle" slot between the order
    // header and the totals/Place Order footer, just with centered empty-
    // state content instead of rows.
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 text-slate-300">
        <ClipboardList className="h-10 w-10" />
        <p className="text-sm font-medium text-slate-400">No Item Selected</p>
      </div>
    );
  }

  return (
    // flex-1 + min-h-0 is what makes this the checkout column's flexible
    // "middle": it claims exactly the vertical space left over after the
    // order header above and the totals/discount/payment/Place Order block
    // below (siblings in BillingPage's flex-col, neither of them flex-1) —
    // so that footer always sits flush against the panel's bottom edge, 1
    // item or 15. A short cart just leaves this area mostly empty below the
    // last row, which is fine; `min-h-0` is the part that actually lets it
    // shrink below its own content height so `overflow-y-auto` can kick in
    // and scroll internally for a long cart, instead of growing past its
    // slot and pushing the footer down/off-panel (flex items default to
    // `min-height: auto`, which would otherwise block that shrink).
    // No `divide-y` here — each `CartRow` now owns its own divider as part
    // of its border color (see its doc comment); a parent-level `divide-y`
    // border-top competes with a row's own border and wins on specificity,
    // which was producing the broken active-row top-left corner.
    <ul className="min-h-0 flex-1 overflow-y-auto">
      {cartOrder.map((itemId) => (
        <CartRow key={itemId} itemId={itemId} currency={currency} onEditLine={onEditLine} />
      ))}
    </ul>
  );
}
