import { useBillingStore } from "../../store";

/** Sits where the reference's "Add Promo or Voucher" row does — same actual
 * discount mechanism as before (flat amount or percent off), just restyled
 * to read as one pill-shaped row instead of a labeled form field. */
export function DiscountControl() {
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);
  const setDiscountMode = useBillingStore((state) => state.setDiscountMode);
  const setDiscountValue = useBillingStore((state) => state.setDiscountValue);

  return (
    <div className="flex items-center gap-2 rounded-full bg-slate-50 py-1 pl-4 pr-1.5">
      <input
        type="number"
        min={0}
        step={discountMode === "percent" ? 1 : 0.01}
        max={discountMode === "percent" ? 100 : undefined}
        value={discountValue || ""}
        onChange={(e) => setDiscountValue(Number(e.target.value) || 0)}
        placeholder="Add Discount"
        className="w-full bg-transparent text-sm text-slate-700 placeholder:text-slate-400 focus:outline-none"
      />
      <div className="flex shrink-0 rounded-full bg-white p-0.5 shadow-soft">
        {(["flat", "percent"] as const).map((mode) => (
          <button
            key={mode}
            type="button"
            onClick={() => setDiscountMode(mode)}
            className={[
              "rounded-full px-2.5 py-1 text-xs font-semibold transition-colors",
              discountMode === mode ? "bg-brand-600 text-white" : "text-slate-500 hover:text-slate-700",
            ].join(" ")}
          >
            {mode === "flat" ? "Amt" : "%"}
          </button>
        ))}
      </div>
    </div>
  );
}
