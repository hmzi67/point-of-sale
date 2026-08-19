import { Banknote, CreditCard, Wallet } from "lucide-react";
import { useBillingStore } from "../../store";
import type { PaymentMethod } from "../../types";

const METHODS: { value: PaymentMethod; label: string; icon: typeof Banknote }[] = [
  { value: "cash", label: "Cash", icon: Banknote },
  { value: "card", label: "Card", icon: CreditCard },
  { value: "other", label: "Other", icon: Wallet },
];

export function PaymentMethodSelector() {
  const paymentMethod = useBillingStore((state) => state.paymentMethod);
  const setPaymentMethod = useBillingStore((state) => state.setPaymentMethod);

  return (
    <div>
      <span className="block text-xs font-medium text-slate-500">Payment method</span>
      <div className="mt-1 grid grid-cols-3 gap-2">
        {METHODS.map(({ value, label, icon: Icon }) => (
          <button
            key={value}
            type="button"
            onClick={() => setPaymentMethod(value)}
            className={[
              "flex flex-col items-center gap-1 rounded-md border py-2 text-xs font-medium transition-colors",
              paymentMethod === value
                ? "border-brand-500 bg-brand-50 text-brand-700"
                : "border-slate-200 text-slate-500 hover:bg-slate-50",
            ].join(" ")}
          >
            <Icon className="h-4 w-4" />
            {label}
          </button>
        ))}
      </div>
    </div>
  );
}
