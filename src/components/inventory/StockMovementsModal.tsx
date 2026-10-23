import { useEffect, useState } from "react";
import { listStockMovements } from "../../services/stockService";
import type { MovementReason, StockMovement } from "../../types";
import { formatMinor, formatQty } from "../../utils/format";
import { ModalCard } from "../ui/ModalCard";
import { DateRangeInputs } from "../ui/DateRangeInputs";

const REASONS: { value: MovementReason; label: string }[] = [
  { value: "grn", label: "Goods received" },
  { value: "add", label: "Stock added" },
  { value: "damage", label: "Damaged / lost" },
  { value: "correction", label: "Count corrected" },
  { value: "stock_take", label: "Stock take" },
];

export const reasonLabel = (reason: MovementReason) => REASONS.find((r) => r.value === reason)?.label ?? reason;

/** Every non-sale stock change — goods in, damage claims, corrections and
 * stock-take variances — newest first. */
export function StockMovementsModal({ currency, itemId = null, onClose }: { currency: string; itemId?: number | null; onClose: () => void }) {
  const [reason, setReason] = useState<MovementReason | "">("");
  const [startDate, setStartDate] = useState("");
  const [endDate, setEndDate] = useState("");
  const [rows, setRows] = useState<StockMovement[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listStockMovements({ itemId, reason: reason || null, startDate: startDate || null, endDate: endDate || null })
      .then((list) => {
        setRows(list);
        setError(null);
      })
      .catch((e: Error) => setError(e.message));
  }, [itemId, reason, startDate, endDate]);

  return (
    <ModalCard title="Stock movements" subtitle="Goods received, damage write-offs, corrections and counts" width="xl" onClose={onClose}>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <select value={reason} onChange={(e) => setReason(e.target.value as MovementReason | "")} className="rounded-xl border border-line-strong py-2 pl-3 pr-8 text-sm">
          <option value="">All movements</option>
          {REASONS.map((r) => (
            <option key={r.value} value={r.value}>{r.label}</option>
          ))}
        </select>
        <DateRangeInputs
          start={startDate}
          end={endDate}
          onStartChange={setStartDate}
          onEndChange={setEndDate}
          inputClassName="rounded-xl border border-line-strong px-3 py-2 text-sm"
        />
      </div>
      {error && <p className="mb-3 text-xs text-danger-text">{error}</p>}
      <div className="overflow-x-auto rounded-xl border border-line">
        <table className="w-full text-sm">
          <thead className="bg-surface-2 text-left text-xs text-fg-muted">
            <tr>
              <th className="px-3 py-2 font-medium">When</th>
              <th className="px-3 py-2 font-medium">Item</th>
              <th className="px-3 py-2 font-medium">Type</th>
              <th className="px-3 py-2 text-right font-medium">Change</th>
              <th className="px-3 py-2 text-right font-medium">Value</th>
              <th className="px-3 py-2 font-medium">Note</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line tabular-nums">
            {rows.map((m) => (
              <tr key={m.id}>
                <td className="whitespace-nowrap px-3 py-2 text-xs text-fg-muted">{m.createdAt}{m.userName && <span className="block">{m.userName}</span>}</td>
                <td className="px-3 py-2 text-fg">{m.itemName}</td>
                <td className="px-3 py-2 text-fg-2">
                  {reasonLabel(m.reason)}
                  {m.referenceId !== null && <span className="text-xs text-fg-muted"> #{m.referenceId}</span>}
                </td>
                <td className={`px-3 py-2 text-right font-semibold ${m.qtyChange < 0 ? "text-danger-text" : "text-success-text"}`}>
                  {m.qtyChange > 0 ? "+" : ""}{formatQty(m.qtyChange, m.unit)}
                </td>
                <td className="px-3 py-2 text-right">{formatMinor(m.valueMinor, currency)}</td>
                <td className="px-3 py-2 text-xs text-fg-muted">{m.note ?? ""}</td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr><td colSpan={6} className="px-3 py-8 text-center text-xs text-fg-muted">No stock movements.</td></tr>
            )}
          </tbody>
        </table>
      </div>
    </ModalCard>
  );
}
