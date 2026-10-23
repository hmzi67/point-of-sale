import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { createPurchaseOrder, updatePurchaseOrder } from "../../services/purchasingService";
import type { Item, PurchaseOrder, Supplier } from "../../types";
import { todayString } from "../../utils/dateRanges";
import { decimalToMinor, formatMinor, minorToDecimal } from "../../utils/format";
import { runAction } from "../../utils/runAction";
import { Field, inputClass, ModalCard, primaryButtonClass, secondaryButtonClass } from "../ui/ModalCard";
import { ItemSelect } from "./ItemSelect";

interface Line {
  itemId: number | null;
  qty: string;
  unitCost: string;
}

/** Creates or edits a draft purchase order; "Place order" sends it. */
export function PurchaseOrderModal({
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
  const [orderDate, setOrderDate] = useState(order?.orderDate ?? todayString());
  const [expectedDate, setExpectedDate] = useState(order?.expectedDate ?? "");
  const [notes, setNotes] = useState(order?.notes ?? "");
  const [lines, setLines] = useState<Line[]>(
    order?.lines.map((l) => ({ itemId: l.itemId, qty: String(l.qtyOrdered), unitCost: String(minorToDecimal(l.unitCostMinor)) })) ?? [
      { itemId: null, qty: "1", unitCost: "" },
    ],
  );
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const update = (index: number, patch: Partial<Line>) => setLines(lines.map((l, i) => (i === index ? { ...l, ...patch } : l)));
  const total = lines.reduce((sum, l) => sum + Math.round(decimalToMinor(Number(l.unitCost) || 0) * (Number(l.qty) || 0)), 0);

  const save = (place: boolean) =>
    runAction(setIsSaving, setError, async () => {
      if (!supplierId) throw new Error("Choose a supplier");
      const valid = lines.filter((l) => l.itemId !== null && Number(l.qty) > 0);
      if (valid.length === 0) throw new Error("Add at least one item");
      const input = {
        supplierId,
        orderDate,
        expectedDate: expectedDate || null,
        notes: notes.trim() || null,
        lines: valid.map((l) => ({ itemId: l.itemId as number, qty: Number(l.qty), unitCostMinor: decimalToMinor(Number(l.unitCost) || 0) })),
        place,
      };
      if (order) await updatePurchaseOrder(order.id, input);
      else await createPurchaseOrder(input);
      onSaved();
    });

  return (
    <ModalCard title={order ? `Purchase order #${order.id}` : "New purchase order"} width="lg" onClose={onClose}>
      <div className="space-y-4">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Field label="Supplier">
            <select value={supplierId ?? ""} onChange={(e) => setSupplierId(Number(e.target.value))} className={`${inputClass} pr-8`}>
              {suppliers.map((s) => (
                <option key={s.id} value={s.id}>{s.name}</option>
              ))}
            </select>
          </Field>
          <Field label="Order date">
            <input type="date" value={orderDate} onChange={(e) => setOrderDate(e.target.value)} className={inputClass} />
          </Field>
          <Field label="Expected delivery">
            <input type="date" value={expectedDate} onChange={(e) => setExpectedDate(e.target.value)} className={inputClass} />
          </Field>
        </div>

        <div className="space-y-2">
          <div className="grid grid-cols-[1fr_6rem_7rem_7rem_2rem] gap-2 text-xs font-medium text-fg-muted">
            <span>Item</span>
            <span>Qty</span>
            <span>Unit cost</span>
            <span className="text-right">Line total</span>
            <span />
          </div>
          {lines.map((line, index) => (
            <div key={index} className="grid grid-cols-[1fr_6rem_7rem_7rem_2rem] items-center gap-2">
              <ItemSelect
                items={items}
                value={line.itemId}
                onChange={(item) => update(index, { itemId: item.id, unitCost: line.unitCost || String(minorToDecimal(item.costMinor)) })}
              />
              <input type="number" min="0" step="any" value={line.qty} onChange={(e) => update(index, { qty: e.target.value })} className="rounded-lg border border-line-strong px-2 py-2 text-sm" />
              <input type="number" min="0" step="0.01" value={line.unitCost} onChange={(e) => update(index, { unitCost: e.target.value })} className="rounded-lg border border-line-strong px-2 py-2 text-sm" />
              <span className="text-right text-sm tabular-nums text-fg">
                {formatMinor(Math.round(decimalToMinor(Number(line.unitCost) || 0) * (Number(line.qty) || 0)), currency)}
              </span>
              <button type="button" onClick={() => setLines(lines.filter((_, i) => i !== index))} aria-label="Remove line" className="text-fg-muted hover:text-danger-text">
                <Trash2 className="h-4 w-4" />
              </button>
            </div>
          ))}
          <div className="flex items-center justify-between">
            <button type="button" onClick={() => setLines([...lines, { itemId: null, qty: "1", unitCost: "" }])} className="flex items-center gap-1 text-sm font-medium text-accent-text hover:underline">
              <Plus className="h-4 w-4" /> Add item
            </button>
            <span className="text-sm font-semibold tabular-nums text-fg">Total {formatMinor(total, currency)}</span>
          </div>
        </div>

        <Field label="Notes">
          <input value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Optional" className={inputClass} />
        </Field>
        {error && <p className="text-xs text-danger-text">{error}</p>}
        <div className="flex flex-wrap justify-end gap-2">
          <button type="button" onClick={onClose} className={secondaryButtonClass}>Cancel</button>
          <button type="button" disabled={isSaving} onClick={() => void save(false)} className={secondaryButtonClass}>
            Save draft
          </button>
          <button type="button" disabled={isSaving} onClick={() => void save(true)} className={primaryButtonClass}>
            {isSaving ? "Saving…" : "Place order"}
          </button>
        </div>
      </div>
    </ModalCard>
  );
}
