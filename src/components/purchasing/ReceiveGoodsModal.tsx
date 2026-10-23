import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { receiveGoods } from "../../services/purchasingService";
import type { Item, PurchaseOrder, Supplier } from "../../types";
import { todayString } from "../../utils/dateRanges";
import { decimalToMinor, formatMinor, minorToDecimal } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";
import { ItemSelect } from "./ItemSelect";

interface Line {
  itemId: number | null;
  poItemId: number | null;
  qty: string;
  unitCost: string;
  batchNo: string;
  expiryDate: string;
}

/** A goods received note — against a placed purchase order (its
 * outstanding lines pre-filled) or a direct delivery. Saving adds the stock
 * and updates each item's cost. */
export function ReceiveGoodsModal({
  order,
  suppliers,
  items,
  currency,
  onSaved,
  onClose,
}: {
  order: PurchaseOrder | null;
  suppliers: Supplier[];
  items: Item[];
  currency: string;
  onSaved: () => void;
  onClose: () => void;
}) {
  const [supplierId, setSupplierId] = useState<number | null>(order?.supplierId ?? suppliers[0]?.id ?? null);
  const [receivedDate, setReceivedDate] = useState(todayString());
  const [invoiceRef, setInvoiceRef] = useState("");
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<Line[]>(
    order
      ? order.lines
          .filter((l) => l.qtyReceived < l.qtyOrdered)
          .map((l) => ({
            itemId: l.itemId,
            poItemId: l.id,
            qty: String(Math.round((l.qtyOrdered - l.qtyReceived) * 1000) / 1000),
            unitCost: String(minorToDecimal(l.unitCostMinor)),
            batchNo: "",
            expiryDate: "",
          }))
      : [{ itemId: null, poItemId: null, qty: "1", unitCost: "", batchNo: "", expiryDate: "" }],
  );
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const update = (index: number, patch: Partial<Line>) => setLines(lines.map((l, i) => (i === index ? { ...l, ...patch } : l)));
  const total = lines.reduce((sum, l) => sum + Math.round(decimalToMinor(Number(l.unitCost) || 0) * (Number(l.qty) || 0)), 0);
  const itemName = (id: number | null) => items.find((i) => i.id === id)?.name ?? "—";

  const save = () =>
    runAction(setIsSaving, setError, async () => {
      if (!supplierId) throw new Error("Choose a supplier");
      const valid = lines.filter((l) => l.itemId !== null && Number(l.qty) > 0);
      if (valid.length === 0) throw new Error("Add at least one received item");
      await receiveGoods({
        supplierId,
        poId: order?.id ?? null,
        receivedDate,
        invoiceRef: invoiceRef.trim() || null,
        notes: notes.trim() || null,
        lines: valid.map((l) => ({
          itemId: l.itemId as number,
          poItemId: l.poItemId,
          qty: Number(l.qty),
          unitCostMinor: decimalToMinor(Number(l.unitCost) || 0),
          batchNo: l.batchNo.trim() || null,
          expiryDate: l.expiryDate || null,
        })),
      });
      onSaved();
    });

  return (
    <ModalCard
      title={order ? `Receive goods — PO #${order.id}` : "Receive goods (GRN)"}
      subtitle="Stock goes up and each item's cost becomes the unit cost received."
      width="xl"
      onClose={onClose}
    >
      <div className="space-y-4">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Field label="Supplier">
            <select value={supplierId ?? ""} disabled={order !== null} onChange={(e) => setSupplierId(Number(e.target.value))} className={`${inputClass} pr-8`}>
              {suppliers.map((s) => (
                <option key={s.id} value={s.id}>{s.name}</option>
              ))}
            </select>
          </Field>
          <Field label="Received on">
            <input type="date" value={receivedDate} onChange={(e) => setReceivedDate(e.target.value)} className={inputClass} />
          </Field>
          <Field label="Supplier invoice no.">
            <input value={invoiceRef} onChange={(e) => setInvoiceRef(e.target.value)} placeholder="Optional" className={inputClass} />
          </Field>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full min-w-[44rem] text-sm">
            <thead className="text-left text-xs text-fg-muted">
              <tr>
                <th className="pb-2 font-medium">Item</th>
                <th className="w-24 pb-2 font-medium">Qty</th>
                <th className="w-28 pb-2 font-medium">Unit cost</th>
                <th className="w-28 pb-2 font-medium">Batch</th>
                <th className="w-36 pb-2 font-medium">Expiry</th>
                <th className="w-8" />
              </tr>
            </thead>
            <tbody>
              {lines.map((line, index) => (
                <tr key={index}>
                  <td className="py-1 pr-2">
                    {line.poItemId !== null ? (
                      <span className="font-medium text-fg">{itemName(line.itemId)}</span>
                    ) : (
                      <ItemSelect
                        items={items}
                        value={line.itemId}
                        onChange={(item) => update(index, { itemId: item.id, unitCost: line.unitCost || String(minorToDecimal(item.costMinor)) })}
                      />
                    )}
                  </td>
                  <td className="py-1 pr-2">
                    <input type="number" min="0" step="any" value={line.qty} onChange={(e) => update(index, { qty: e.target.value })} className="w-full rounded-lg border border-line-strong px-2 py-2" />
                  </td>
                  <td className="py-1 pr-2">
                    <input type="number" min="0" step="0.01" value={line.unitCost} onChange={(e) => update(index, { unitCost: e.target.value })} className="w-full rounded-lg border border-line-strong px-2 py-2" />
                  </td>
                  <td className="py-1 pr-2">
                    <input value={line.batchNo} onChange={(e) => update(index, { batchNo: e.target.value })} placeholder="—" className="w-full rounded-lg border border-line-strong px-2 py-2" />
                  </td>
                  <td className="py-1 pr-2">
                    <input type="date" value={line.expiryDate} onChange={(e) => update(index, { expiryDate: e.target.value })} className="w-full rounded-lg border border-line-strong px-2 py-2" />
                  </td>
                  <td className="py-1">
                    <button type="button" onClick={() => setLines(lines.filter((_, i) => i !== index))} aria-label="Remove line" className="text-fg-muted hover:text-danger-text">
                      <Trash2 className="h-4 w-4" />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="flex items-center justify-between">
          <button
            type="button"
            onClick={() => setLines([...lines, { itemId: null, poItemId: null, qty: "1", unitCost: "", batchNo: "", expiryDate: "" }])}
            className="flex items-center gap-1 text-sm font-medium text-accent-text hover:underline"
          >
            <Plus className="h-4 w-4" /> Add item
          </button>
          <span className="text-sm font-semibold tabular-nums text-fg">Total {formatMinor(total, currency)}</span>
        </div>
        <Field label="Notes">
          <input value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Optional" className={inputClass} />
        </Field>
        {error && <p className="text-xs text-danger-text">{error}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>Cancel</button>
          <button type="button" disabled={isSaving} onClick={() => void save()} className={primaryButtonClass}>
            {isSaving ? "Saving…" : "Receive into stock"}
          </button>
        </div>
      </div>
    </ModalCard>
  );
}
