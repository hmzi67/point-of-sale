import { ShoppingCart } from "lucide-react";
import { useBillingStore } from "../../store";
import { CartRow } from "./CartRow";

interface CartPanelProps {
  currency: string;
}

/** Maps over the stable `cartOrder` id list — this only changes on add/remove,
 * so an in-place quantity edit never re-renders the list itself, only the one
 * `CartRow` whose entry changed. */
export function CartPanel({ currency }: CartPanelProps) {
  const cartOrder = useBillingStore((state) => state.cartOrder);

  if (cartOrder.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 text-slate-400">
        <ShoppingCart className="h-8 w-8" />
        <p className="text-sm">Cart is empty — scan or search an item to begin</p>
      </div>
    );
  }

  return (
    <ul className="flex-1 divide-y divide-slate-100 overflow-y-auto">
      {cartOrder.map((itemId) => (
        <CartRow key={itemId} itemId={itemId} currency={currency} />
      ))}
    </ul>
  );
}
