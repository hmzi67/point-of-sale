import { useBillingStore } from "../../store";

export function DiscountControl() {
  const discountMode = useBillingStore((state) => state.discountMode);
  const discountValue = useBillingStore((state) => state.discountValue);
  const setDiscountMode = useBillingStore((state) => state.setDiscountMode);
  const setDiscountValue = useBillingStore((state) => state.setDiscountValue);

  return (
    <div>
      <label className="block text-xs font-medium text-slate-500" htmlFor="discount-value">
        Discount
      </label>
      <div className="mt-1 flex gap-2">
        <div className="flex rounded-md border border-slate-300 p-0.5">
          {(["flat", "percent"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              onClick={() => setDiscountMode(mode)}
              className={[
                "rounded px-2.5 py-1 text-xs font-medium transition-colors",
                discountMode === mode ? "bg-brand-600 text-white" : "text-slate-500 hover:bg-slate-100",
              ].join(" ")}
            >
              {mode === "flat" ? "Amount" : "%"}
            </button>
          ))}
        </div>
        <input
          id="discount-value"
          type="number"
          min={0}
          step={discountMode === "percent" ? 1 : 0.01}
          max={discountMode === "percent" ? 100 : undefined}
          value={discountValue || ""}
          onChange={(e) => setDiscountValue(Number(e.target.value) || 0)}
          placeholder="0"
          className="w-full rounded-md border border-slate-300 px-3 py-1.5 text-sm"
        />
      </div>
    </div>
  );
}
