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
    <div className="flex items-center gap-2 rounded-xl border border-line bg-surface-2/50 py-2 pl-3 pr-2 focus-within:border-accent/40">
      <input
        type="number"
        min={0}
        step={discountMode === "percent" ? 1 : 0.01}
        max={discountMode === "percent" ? 100 : undefined}
        value={discountValue || ""}
        onChange={(e) => setDiscountValue(Number(e.target.value) || 0)}
        placeholder="Add discount"
        aria-label="Order discount"
        // [appearance:textfield] + the two -webkit-* rules drop the native
        // spin-button arrows — at this pill's width they only ever crowded
        // the digits/placeholder against the Amt/% toggle, never gave a
        // usable click target.
        className="min-w-0 flex-1 bg-transparent text-sm text-fg-2 placeholder:text-fg-muted focus:outline-none [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
      />
      <div className="flex shrink-0 rounded-lg bg-surface p-0.5 shadow-soft">
        {(["flat", "percent"] as const).map((mode) => (
          <button
            key={mode}
            type="button"
            onClick={() => setDiscountMode(mode)}
            aria-pressed={discountMode === mode}
            className={[
              "rounded-lg px-2.5 py-1 text-xs font-semibold transition-colors",
              discountMode === mode ? "bg-accent text-accent-ink" : "text-fg-muted hover:text-fg-2",
            ].join(" ")}
          >
            {mode === "flat" ? "Amt" : "%"}
          </button>
        ))}
      </div>
    </div>
  );
}
