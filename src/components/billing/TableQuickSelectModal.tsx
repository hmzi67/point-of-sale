import { useEffect, useRef, useState } from "react";
import { getTables } from "../../services/tablesService";
import { useBillingStore } from "../../store";

interface TableQuickSelectModalProps {
  onClose: () => void;
}

/** Pulls the table number out of a table's display name — "Table 30" → 30,
 * as does a bare "30" or a custom name like "VIP 7" → 7 (its last run of
 * digits). Tables are free-named (see `tablesService.addTable`/`renameTable`,
 * Owner/Admin can call a table anything), so this can't assume the seeded
 * "Table N" convention holds forever — matching on the trailing digits is
 * the same trade-off Phase 6's real table-number field is expected to
 * eventually replace (see `CLAUDE.md`'s note on `table_orders.cart_json`
 * being a deliberate stand-in). Returns `null` for a name with no digits at
 * all, which never matches a typed number. */
function tableNumberFromName(name: string): number | null {
  const match = name.match(/(\d+)\s*$/);
  if (!match) return null;
  return Number(match[1]);
}

/**
 * Pressing `T` on the billing surface (see `useFastBillingHotkeys`) opens
 * this instead of the mouse dropdown — a numeric input, already focused, so
 * a cashier can type a table number and hit Enter with no click at all.
 * Deliberately lightweight: no fetch-on-every-keystroke, no animation delay,
 * just a small centered card. Confirming has the exact same effect as
 * picking the table from `OrderTypeAndTable`'s dropdown (`setTableId`) —
 * this is a faster path to the same state, not a parallel one.
 */
export function TableQuickSelectModal({ onClose }: TableQuickSelectModalProps) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const setTableId = useBillingStore((state) => state.setTableId);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const confirm = async () => {
    const typed = value.trim();
    if (!typed) return;
    const wanted = Number(typed);

    try {
      const tables = await getTables();
      const match = tables.find((table) => tableNumberFromName(table.name) === wanted);
      if (!match) {
        setError(`Table ${typed} doesn't exist`);
        return;
      }
      setTableId(match.id);
      onClose();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/30"
      onClick={onClose}
    >
      <div
        className="w-full max-w-[16rem] rounded-2xl bg-white p-4 shadow-soft-lg"
        onClick={(e) => e.stopPropagation()}
      >
        <label className="block text-xs font-medium text-slate-500" htmlFor="quick-table-number">
          Go to table
        </label>
        <input
          id="quick-table-number"
          ref={inputRef}
          value={value}
          onChange={(e) => {
            setValue(e.target.value.replace(/\D/g, "").slice(0, 4));
            setError(null);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void confirm();
            } else if (e.key === "Escape") {
              e.preventDefault();
              onClose();
            }
          }}
          inputMode="numeric"
          placeholder="Table number…"
          className="mt-1.5 w-full rounded-xl border border-slate-300 px-3 py-2 text-base focus:outline-none focus:ring-2 focus:ring-brand-200"
        />
        {error && <p className="mt-1.5 text-xs text-red-600">{error}</p>}
        <p className="mt-2 text-[11px] text-slate-400">Enter to select · Esc to cancel</p>
      </div>
    </div>
  );
}
