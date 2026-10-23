import { useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import { getItems } from "../../services/inventoryService";
import { getStockTake, listStockTakes, submitStockTake } from "../../services/stockService";
import type { Item, StockTake } from "../../types";
import { formatMinor, formatQty } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";

/** Count what's on the shelf. Only items given a count are touched: each
 * one's stock becomes the counted figure and the difference is logged. */
export function StockTakeModal({ currency, onSaved, onClose }: { currency: string; onSaved: () => void; onClose: () => void }) {
  const [items, setItems] = useState<Item[]>([]);
  const [counts, setCounts] = useState<Record<number, string>>({});
  const [search, setSearch] = useState("");
  const [note, setNote] = useState("");
  const [history, setHistory] = useState<StockTake[]>([]);
  const [result, setResult] = useState<StockTake | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([getItems({}), listStockTakes()])
      .then(([list, takes]) => {
        setItems(list);
        setHistory(takes);
      })
      .catch((e: Error) => setError(e.message));
  }, []);

  const visible = useMemo(() => {
    const q = search.trim().toLowerCase();
    return q ? items.filter((i) => i.name.toLowerCase().includes(q) || i.barcode?.includes(q)) : items;
  }, [items, search]);
  const counted = Object.entries(counts).filter(([, v]) => v.trim() !== "" && Number(v) >= 0);

  const submit = () =>
    runAction(setIsSaving, setError, async () => {
      const take = await submitStockTake(
        note.trim() || null,
        counted.map(([id, v]) => ({ itemId: Number(id), countedQty: Number(v) })),
      );
      setResult(take);
      onSaved();
    });

  if (result) {
    return (
      <ModalCard title={`Stock take #${result.id} saved`} subtitle={`${result.itemsCounted} counted · ${result.itemsWithVariance} with a difference`} width="lg" onClose={onClose}>
        <StockTakeLines take={result} currency={currency} />
        <div className="mt-4 flex justify-end">
          <button type="button" onClick={onClose} className={primaryButtonClass}>Done</button>
        </div>
      </ModalCard>
    );
  }

  return (
    <ModalCard title="Stock take" subtitle="Enter the counted quantity for the items you've checked" width="xl" onClose={onClose}>
      <div className="space-y-3">
        <div className="flex flex-wrap gap-2">
          <div className="relative min-w-56 flex-1">
            <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
            <input value={search} onChange={(e) => setSearch(e.target.value)} placeholder="Find item or scan barcode" className={`${inputClass} pl-9`} />
          </div>
          <input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Note (e.g. month-end count)" className={`${inputClass} max-w-xs`} />
        </div>
        <div className="max-h-[50vh] overflow-y-auto rounded-xl border border-line">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-surface-2 text-left text-xs text-fg-muted">
              <tr>
                <th className="px-3 py-2 font-medium">Item</th>
                <th className="px-3 py-2 text-right font-medium">System</th>
                <th className="w-32 px-3 py-2 font-medium">Counted</th>
                <th className="px-3 py-2 text-right font-medium">Difference</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line tabular-nums">
              {visible.map((item) => {
                const value = counts[item.id] ?? "";
                const diff = value.trim() === "" ? null : Number(value) - item.stockQty;
                return (
                  <tr key={item.id}>
                    <td className="px-3 py-2 text-fg">{item.name}<span className="block text-xs text-fg-muted">{item.barcode ?? ""}</span></td>
                    <td className="px-3 py-2 text-right text-fg-2">{formatQty(item.stockQty, item.unit)}</td>
                    <td className="px-3 py-1.5">
                      <input
                        type="number"
                        min="0"
                        step="any"
                        value={value}
                        onChange={(e) => setCounts({ ...counts, [item.id]: e.target.value })}
                        className="w-full rounded-lg border border-line-strong px-2 py-1.5"
                      />
                    </td>
                    <td className={`px-3 py-2 text-right font-semibold ${diff === null || diff === 0 ? "text-fg-muted" : diff < 0 ? "text-danger-text" : "text-success-text"}`}>
                      {diff === null ? "" : `${diff > 0 ? "+" : ""}${formatQty(diff, item.unit)}`}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
        {error && <p className="text-xs text-danger-text">{error}</p>}
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs text-fg-muted">{counted.length} item(s) counted</span>
          <div className="flex gap-2">
            <button type="button" onClick={onClose} className={secondaryButtonClass}>Cancel</button>
            <button type="button" disabled={isSaving || counted.length === 0} onClick={() => void submit()} className={primaryButtonClass}>
              {isSaving ? "Saving…" : "Save count"}
            </button>
          </div>
        </div>

        {history.length > 0 && (
          <div className="border-t border-line pt-3">
            <h4 className="mb-2 text-xs font-semibold text-fg-2">Previous stock takes</h4>
            <ul className="space-y-1 text-sm">
              {history.slice(0, 8).map((take) => (
                <li key={take.id}>
                  <button
                    type="button"
                    onClick={() => getStockTake(take.id).then(setResult).catch((e: Error) => setError(e.message))}
                    className="flex w-full justify-between rounded-lg px-2 py-1.5 text-left hover:bg-surface-2"
                  >
                    <span className="text-fg">#{take.id} · {take.createdAt}{take.note ? ` · ${take.note}` : ""}</span>
                    <span className={`tabular-nums ${take.varianceValueMinor < 0 ? "text-danger-text" : "text-fg-muted"}`}>
                      {take.itemsWithVariance}/{take.itemsCounted} off · {formatMinor(take.varianceValueMinor, currency)}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </ModalCard>
  );
}

function StockTakeLines({ take, currency }: { take: StockTake; currency: string }) {
  return (
    <div className="overflow-x-auto rounded-xl border border-line">
      <table className="w-full text-sm">
        <thead className="bg-surface-2 text-left text-xs text-fg-muted">
          <tr>
            <th className="px-3 py-2 font-medium">Item</th>
            <th className="px-3 py-2 text-right font-medium">Expected</th>
            <th className="px-3 py-2 text-right font-medium">Counted</th>
            <th className="px-3 py-2 text-right font-medium">Variance</th>
            <th className="px-3 py-2 text-right font-medium">Value</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line tabular-nums">
          {take.lines.map((line) => (
            <tr key={line.itemId}>
              <td className="px-3 py-2 text-fg">{line.itemName}</td>
              <td className="px-3 py-2 text-right">{formatQty(line.expectedQty, line.unit)}</td>
              <td className="px-3 py-2 text-right">{formatQty(line.countedQty, line.unit)}</td>
              <td className={`px-3 py-2 text-right font-semibold ${line.varianceQty < 0 ? "text-danger-text" : line.varianceQty > 0 ? "text-success-text" : "text-fg-muted"}`}>
                {line.varianceQty > 0 ? "+" : ""}{formatQty(line.varianceQty, line.unit)}
              </td>
              <td className="px-3 py-2 text-right">{formatMinor(line.varianceValueMinor, currency)}</td>
            </tr>
          ))}
        </tbody>
        <tfoot>
          <tr className="border-t border-line font-semibold">
            <td className="px-3 py-2" colSpan={4}>Net variance</td>
            <td className="px-3 py-2 text-right tabular-nums">{formatMinor(take.varianceValueMinor, currency)}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  );
}
