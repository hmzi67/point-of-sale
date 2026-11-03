import { useState } from "react";
import { Check } from "lucide-react";
import { useAppStore } from "../../store";
import type { AppConfig, Sale } from "../../types";
import { runAction } from "../../utils/runAction";
import { ReceiptPaper } from "../billing/ReceiptPaper";

const line = (itemId: number, itemName: string, qty: number, rateMinor: number) => ({
  itemId,
  itemName,
  qty,
  unit: null,
  priceAtSaleMinor: rateMinor,
  lineTotalMinor: qty * rateMinor,
  notes: null,
});

/** Example data only: what a design looks like, with this shop's own header. */
const SAMPLE_SALE = {
  id: 0,
  receiptNo: "1110",
  orderNo: 1,
  cashierName: "Manager",
  createdAt: "2026-01-20 16:02:00",
  orderType: "dine_in",
  tableName: "Table 1",
  items: [line(1, "Product 1", 3, 13000), line(2, "Product 2", 6, 25000), line(3, "Product 3", 7, 25000)],
  subtotalMinor: 364000,
  discountMinor: 0,
  taxMinor: 0,
  serviceChargeMinor: 0,
  totalMinor: 364000,
  paymentMethod: "cash",
  payments: [{ method: "cash", amountMinor: 364000 }],
  tenderedMinor: 400000,
  changeMinor: 36000,
} as unknown as Sale;

const DESIGNS: { key: AppConfig["receiptTemplate"]; label: string; hint: string }[] = [
  { key: "stacked", label: "Classic", hint: "Item name on its own line. Long names stay readable." },
  { key: "compact", label: "Compact", hint: "One row per item. Uses less paper." },
];

/** Pick the receipt design: the choice applies to the printed receipt, the
 * saved PDF and the on-screen receipt alike. */
export function ReceiptDesignSection({ config }: { config: AppConfig }) {
  const save = useAppStore((state) => state.save);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  return (
    <div className="rounded-2xl border border-line bg-surface p-6">
      <h2 className="text-base font-semibold tracking-tight text-fg">Receipt design</h2>
      <p className="mt-1 text-xs text-fg-muted">The one you select is used everywhere: print, PDF and the receipt on screen.</p>
      {error && <p className="mt-3 text-sm text-danger-text">{error}</p>}

      <div className="mt-5 grid gap-4 md:grid-cols-2">
        {DESIGNS.map(({ key, label, hint }) => {
          const selected = config.receiptTemplate === key;
          return (
            <div key={key} className={`flex flex-col gap-3 rounded-2xl border p-4 ${selected ? "border-accent ring-1 ring-accent" : "border-line"}`}>
              <div>
                <p className="text-sm font-semibold text-fg">{label}</p>
                <p className="text-xs text-fg-muted">{hint}</p>
              </div>
              <div className="flex-1 rounded-xl bg-surface-2 p-3">
                <ReceiptPaper sale={SAMPLE_SALE} config={{ ...config, receiptTemplate: key }} tablesEnabled logoUrl={null} />
              </div>
              <button
                type="button"
                disabled={selected || isSaving}
                aria-pressed={selected}
                onClick={() => void runAction(setIsSaving, setError, () => save({ receiptTemplate: key }))}
                className={`flex items-center justify-center gap-1.5 rounded-xl px-3.5 py-2.5 text-sm font-semibold ${
                  selected ? "bg-accent-soft text-accent-text" : "bg-accent text-accent-ink hover:bg-accent-hover disabled:opacity-50"
                }`}
              >
                {selected && <Check className="h-4 w-4" />}
                {selected ? "Selected" : "Use this design"}
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
