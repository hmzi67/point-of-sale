import { Banknote, CalendarClock, CreditCard, Split, Wallet } from "lucide-react";
import { useModuleEnabled } from "../../hooks/useModules";
import { useBillingStore } from "../../store";
import type { CheckoutMode } from "../../types";
import { paymentMethodLabel } from "../../utils/checkoutPayments";

const METHODS: { value: CheckoutMode; icon: typeof Banknote }[] = [
  { value: "cash", icon: Banknote },
  { value: "card", icon: CreditCard },
  { value: "other", icon: Wallet },
  { value: "split", icon: Split },
  { value: "credit", icon: CalendarClock },
];

/** Payment choices with a visible selected state. Pay later needs a
 * customer account, so it only appears with the `customers` module on. */
export function PaymentMethodSelector() {
  const customersEnabled = useModuleEnabled("customers");
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const setPaymentMethod = useBillingStore((state) => state.setPaymentMethod);
  const methods = METHODS.filter((m) => customersEnabled || m.value !== "credit");

  return (
    <div className="grid grid-cols-3 gap-2">
      {methods.map(({ value, icon: Icon }) => (
        <button
          key={value}
          type="button"
          onClick={() => setPaymentMethod(value)}
          aria-pressed={paymentMethod === value}
          className={[
            "flex min-h-11 items-center justify-center gap-2 rounded-xl border py-2.5 text-xs font-semibold transition-colors",
            paymentMethod === value ? "border-accent/40 bg-accent-soft text-accent-text" : "border-line bg-surface-2/50 text-fg-muted hover:border-line-strong hover:text-fg-2",
          ].join(" ")}
        >
          <Icon className="h-3.5 w-3.5" />
          {paymentMethodLabel(value)}
        </button>
      ))}
    </div>
  );
}
