import { Banknote, RotateCcw } from "lucide-react";
import { useBillingStore } from "../../store";
import { decimalToMinor, formatMinor, minorToDecimal } from "../../utils/format";

interface CashTenderControlProps {
  currency: string;
  totalMinor: number;
}

export function CashTenderControl({ currency, totalMinor }: CashTenderControlProps) {
  const tenderedAmount = useBillingStore((state) => state.tenderedAmount);
  const setTenderedAmount = useBillingStore((state) => state.setTenderedAmount);

  const totalDecimal = minorToDecimal(totalMinor);
  const currentTenderedMinor = tenderedAmount !== null ? decimalToMinor(tenderedAmount) : totalMinor;

  const changeMinor = Math.max(0, currentTenderedMinor - totalMinor);
  const isShort = currentTenderedMinor < totalMinor;
  const shortMinor = totalMinor - currentTenderedMinor;

  // Generate sensible quick cash preset options (next round 50, 100, 500, 1000, 5000)
  const presets: number[] = [];
  const roundedUp100 = Math.ceil(totalDecimal / 100) * 100;
  const roundedUp500 = Math.ceil(totalDecimal / 500) * 500;
  const roundedUp1000 = Math.ceil(totalDecimal / 1000) * 1000;
  const roundedUp5000 = Math.ceil(totalDecimal / 5000) * 5000;

  for (const val of [roundedUp100, roundedUp500, roundedUp1000, roundedUp5000]) {
    if (val > totalDecimal && !presets.includes(val)) {
      presets.push(val);
    }
  }

  return (
    <div className="flex flex-col gap-2 rounded-xl border border-line bg-surface-2/40 p-2.5 sm:p-3 text-xs">
      <div className="flex items-center justify-between">
        <label htmlFor="cash-received-input" className="flex items-center gap-1.5 font-semibold text-fg">
          <Banknote className="h-4 w-4 text-accent-text" />
          <span>Cash Received</span>
        </label>
        {tenderedAmount !== null && (
          <button
            type="button"
            onClick={() => setTenderedAmount(null)}
            className="flex items-center gap-1 text-[11px] text-fg-muted hover:text-fg"
            title="Reset to exact bill amount"
          >
            <RotateCcw className="h-3 w-3" />
            <span>Exact</span>
          </button>
        )}
      </div>

      <div className="relative flex items-center">
        <input
          id="cash-received-input"
          type="number"
          min={0}
          step="any"
          value={tenderedAmount !== null ? tenderedAmount : ""}
          placeholder={String(totalDecimal || 0)}
          onChange={(e) => {
            const val = e.target.value.trim();
            if (val === "") {
              setTenderedAmount(null);
            } else {
              const num = parseFloat(val);
              setTenderedAmount(isNaN(num) ? null : Math.max(0, num));
            }
          }}
          className="w-full rounded-lg border border-line bg-surface px-3 py-1.5 text-sm font-bold text-fg placeholder:font-normal placeholder:text-fg-muted focus:border-accent focus:outline-none"
        />
        <div className="absolute right-2.5 flex items-center gap-1.5 text-xs font-semibold text-fg-muted">
          <span>{currency}</span>
        </div>
      </div>

      {/* Preset shortcuts */}
      <div className="flex flex-wrap items-center gap-1.5">
        <button
          type="button"
          onClick={() => setTenderedAmount(totalDecimal)}
          className={`rounded-lg px-2 py-1 text-[11px] font-medium transition ${
            tenderedAmount === totalDecimal || tenderedAmount === null
              ? "bg-accent text-accent-ink"
              : "border border-line bg-surface text-fg-muted hover:bg-surface-2"
          }`}
        >
          Exact
        </button>
        {presets.slice(0, 3).map((amt) => (
          <button
            key={amt}
            type="button"
            onClick={() => setTenderedAmount(amt)}
            className={`rounded-lg px-2 py-1 text-[11px] font-medium transition ${
              tenderedAmount === amt
                ? "bg-accent text-accent-ink"
                : "border border-line bg-surface text-fg-muted hover:bg-surface-2"
            }`}
          >
            {amt.toLocaleString()}
          </button>
        ))}
      </div>

      {/* Live return change or short alert */}
      <div
        className={`flex items-center justify-between rounded-lg px-2.5 py-1.5 text-xs font-semibold ${
          isShort
            ? "bg-warning-soft text-warning-text"
            : changeMinor > 0
            ? "bg-success-soft text-success-text"
            : "bg-surface text-fg-muted"
        }`}
      >
        <span>{isShort ? "Remaining Due:" : "Change to Return:"}</span>
        <span className="text-sm font-bold tabular-nums">
          {isShort
            ? formatMinor(shortMinor, currency)
            : formatMinor(changeMinor, currency)}
        </span>
      </div>
    </div>
  );
}
