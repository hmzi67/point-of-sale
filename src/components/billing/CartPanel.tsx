import { ShoppingBag, Plus } from "lucide-react";
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
      <div className="flex min-h-44 flex-1 flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-line bg-surface-2/30 px-5 py-6 text-center">
        <span className="relative flex h-16 w-16 items-center justify-center rounded-2xl bg-accent-soft text-accent-text">
          <ShoppingBag className="h-7 w-7" strokeWidth={1.5} />
          <span className="absolute -bottom-1 -right-1 rounded-full border-4 border-surface bg-accent p-1 text-accent-ink"><Plus className="h-3 w-3" /></span>
        </span>
        <p className="mt-1 text-sm font-semibold text-fg">Your order starts here</p>
        <p className="max-w-52 text-xs leading-relaxed text-fg-muted">Add products from the menu to see your order and total.</p>
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
    // `-mx-2 px-2`: rows keep their text aligned with the panel while the
    // active row's highlight extends 8px into the gutter on each side. The
    // bleed has to be the list's padding (never a row's negative margin) —
    // this is a scroll container, and anything a child pushes outside it
    // is clipped. See `CartRow`'s className comment.
    <ul className="-mx-2 min-h-0 flex-1 space-y-2 overflow-y-auto px-2">
      {cartOrder.map((itemId) => (
        <CartRow key={itemId} itemId={itemId} currency={currency} onEditLine={onEditLine} />
      ))}
    </ul>
  );
}
