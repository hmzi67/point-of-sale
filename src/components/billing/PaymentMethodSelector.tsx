import { Banknote, CreditCard, Wallet } from "lucide-react";
import { useBillingStore } from "../../store";
import type { PaymentMethod } from "../../types";

const METHODS: { value: PaymentMethod; label: string; icon: typeof Banknote }[] = [
  { value: "cash", label: "Cash", icon: Banknote },
  { value: "card", label: "Card", icon: CreditCard },
  { value: "other", label: "Other", icon: Wallet },
];

/** Restyled to sit beside the promo row (reference's "QRIS" pill) — same
 * three payment methods, same store, same value sent to `billing_create_sale`. */
export function PaymentMethodSelector() {
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const setPaymentMethod = useBillingStore((state) => state.setPaymentMethod);

  return (
    <div className="flex gap-1.5 rounded-full bg-surface-2 p-1">
      {METHODS.map(({ value, label, icon: Icon }) => (
        <button
          key={value}
          type="button"
          onClick={() => setPaymentMethod(value)}
          className={[
            "flex flex-1 items-center justify-center gap-1.5 rounded-full py-2 text-xs font-semibold transition-colors",
            paymentMethod === value ? "bg-surface text-accent-text shadow-soft" : "text-fg-muted hover:text-fg-2",
          ].join(" ")}
        >
          <Icon className="h-3.5 w-3.5" />
          {label}
        </button>
      ))}
    </div>
  );
}
