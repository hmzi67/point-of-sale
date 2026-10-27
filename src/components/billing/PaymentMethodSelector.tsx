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
    // One row, icon over label: a second row of methods was what pushed the
    // checkout past the bottom of 768px-tall screens.
    <div className="grid auto-cols-fr grid-flow-col gap-1.5">
      {methods.map(({ value, icon: Icon }) => (
        <button
          key={value}
          type="button"
          onClick={() => setPaymentMethod(value)}
          aria-pressed={paymentMethod === value}
          className={[
            "flex min-h-11 min-w-0 flex-col items-center justify-center gap-1 rounded-xl border px-1 py-1.5 text-[0.6875rem] font-semibold leading-tight whitespace-nowrap transition-colors",
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
