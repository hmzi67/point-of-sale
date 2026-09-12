import { Banknote, CreditCard, Wallet } from "lucide-react";
import { useBillingStore } from "../../store";
import type { PaymentMethod } from "../../types";

const METHODS: { value: PaymentMethod; label: string; icon: typeof Banknote }[] = [
  { value: "cash", label: "Cash", icon: Banknote },
  { value: "card", label: "Card", icon: CreditCard },
  { value: "other", label: "Other", icon: Wallet },
];

/** Three payment choices with a visible selected state. */
export function PaymentMethodSelector() {
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const setPaymentMethod = useBillingStore((state) => state.setPaymentMethod);

  return (
    <div className="flex gap-2">
      {METHODS.map(({ value, label, icon: Icon }) => (
        <button
          key={value}
          type="button"
          onClick={() => setPaymentMethod(value)}
          aria-pressed={paymentMethod === value}
          className={[
            "flex min-h-11 flex-1 items-center justify-center gap-2 rounded-xl border py-2.5 text-xs font-semibold transition-colors",
            paymentMethod === value ? "border-accent/40 bg-accent-soft text-accent-text" : "border-line bg-surface-2/50 text-fg-muted hover:border-line-strong hover:text-fg-2",
          ].join(" ")}
        >
          <Icon className="h-3.5 w-3.5" />
          {label}
        </button>
      ))}
    </div>
  );
}
