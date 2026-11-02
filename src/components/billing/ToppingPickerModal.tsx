import { useEffect, useState } from "react";
import { Check, Layers } from "lucide-react";
import { useAppConfig } from "../../hooks/useAppConfig";
import { getItems } from "../../services/inventoryService";
import { useBillingStore } from "../../store";
import { formatMinor } from "../../utils/format";
import type { Item } from "../../types";
import { ModalCard } from "../ui/ModalCard";
import { ItemImage } from "./ItemImage";

/** Picks toppings for the item just added (`billingStore.toppingPicker`).
 * Each chosen topping is added as its own cart line, so it is priced,
 * stocked, printed and refunded like any other item. Enter adds, Esc skips. */
export function ToppingPickerModal() {
  const picker = useBillingStore((state) => state.toppingPicker);
  const close = useBillingStore((state) => state.closeToppingPicker);
  const addItem = useBillingStore((state) => state.addItem);
  const { config } = useAppConfig();

  const [toppings, setToppings] = useState<Item[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());

  const groupId = picker?.groupId;
  useEffect(() => {
    if (groupId === undefined) return;
    setToppings(null);
    setError(null);
    setSelected(new Set());
    getItems({ categoryId: groupId })
      .then(setToppings)
      .catch((e: Error) => setError(e.message));
  }, [picker, groupId]);

  const chosen = (toppings ?? []).filter((t) => selected.has(t.id));
  const extraMinor = chosen.reduce((sum, t) => sum + t.priceMinor, 0);

  const confirm = () => {
    for (const topping of chosen) addItem(topping);
    close();
  };

  useEffect(() => {
    if (!picker) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Enter" && event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      if (event.key === "Enter") confirm();
      else close();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  });

  if (!picker) return null;

  const toggle = (id: number) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  return (
    <ModalCard title={`Toppings for ${picker.itemName}`} subtitle="Pick any — or skip" icon={Layers} onClose={close}>
      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}
      {toppings === null && !error && <p className="py-8 text-center text-sm text-fg-muted">Loading toppings…</p>}
      {toppings?.length === 0 && (
        <p className="py-8 text-center text-sm text-fg-muted">This topping group has no items yet — add them in Inventory.</p>
      )}

      {toppings && toppings.length > 0 && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(9rem,1fr))] gap-2.5">
          {toppings.map((topping) => {
            const on = selected.has(topping.id);
            const outOfStock = topping.stockQty <= 0;
            return (
              <button
                key={topping.id}
                type="button"
                disabled={outOfStock}
                aria-pressed={on}
                onClick={() => toggle(topping.id)}
                className={`relative flex items-center gap-2.5 rounded-xl border p-2 text-left transition disabled:cursor-not-allowed disabled:opacity-40 ${
                  on ? "border-accent bg-accent-soft" : "border-line bg-surface hover:bg-surface-2"
                }`}
              >
                <ItemImage imagePath={topping.imagePath} alt="" className="h-10 w-10 shrink-0 rounded-lg" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium text-fg">{topping.name}</span>
                  <span className={`block text-xs ${on ? "text-accent-text" : "text-fg-muted"}`}>
                    {outOfStock ? "Out of stock" : `+ ${formatMinor(topping.priceMinor, config.currency)}`}
                  </span>
                </span>
                {on && (
                  <span className="absolute right-1.5 top-1.5 flex h-4 w-4 items-center justify-center rounded-full bg-accent text-accent-ink">
                    <Check className="h-3 w-3" />
                  </span>
                )}
              </button>
            );
          })}
        </div>
      )}

      <div className="mt-4 flex gap-2">
        <button
          type="button"
          onClick={close}
          className="rounded-xl border border-line px-4 py-2.5 text-sm font-medium text-fg-2 hover:bg-surface-2"
        >
          No toppings
        </button>
        <button
          type="button"
          onClick={confirm}
          disabled={chosen.length === 0}
          className="flex flex-1 items-center justify-between gap-2 rounded-xl bg-accent px-4 py-2.5 text-sm font-semibold text-accent-ink hover:bg-accent-hover disabled:opacity-50"
        >
          <span>{chosen.length === 0 ? "Add toppings" : `Add ${chosen.length} topping${chosen.length === 1 ? "" : "s"}`}</span>
          {extraMinor > 0 && <span className="tabular-nums">+ {formatMinor(extraMinor, config.currency)}</span>}
        </button>
      </div>
    </ModalCard>
  );
}
