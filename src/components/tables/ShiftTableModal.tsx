import { useState } from "react";
import { ArrowRightLeft, X } from "lucide-react";
import type { TableSummary } from "../../types";
import { ModalOverlay } from "../ui/ModalOverlay";

interface ShiftTableModalProps {
  fromTable: TableSummary;
  freeTables: TableSummary[];
  onClose: () => void;
  onConfirm: (toTableId: number) => Promise<void>;
}

/** "Shift Table" picker: a customer at `fromTable` moved elsewhere — pick
 * which free table their in-progress order moves to. Only ever lists
 * `free` tables (reused from whatever list the floor view already has, not
 * a fresh fetch) since shifting onto an occupied/reserved one would mean
 * merging two orders — a different, bigger feature this deliberately
 * doesn't attempt. Same fixed-overlay/list/footer shape as `RefundModal`'s
 * "pick one from a list" step. */
export function ShiftTableModal({ fromTable, freeTables, onClose, onConfirm }: ShiftTableModalProps) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const confirm = async () => {
    if (selectedId === null) return;
    setIsSubmitting(true);
    setError(null);
    try {
      await onConfirm(selectedId);
    } catch (e) {
      setError((e as Error).message);
      setIsSubmitting(false);
    }
  };

  return (
    <ModalOverlay>
      <div className="flex max-h-[85dvh] w-full max-w-md flex-col overflow-hidden rounded-3xl bg-surface shadow-soft-lg">
        <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
          <h3 className="flex items-center gap-2 text-sm font-semibold text-fg">
            <ArrowRightLeft className="h-4 w-4 text-accent-text" />
            Shift {fromTable.name} to…
          </h3>
          <button
            type="button"
            onClick={onClose}
            disabled={isSubmitting}
            className="rounded-full p-1 text-fg-muted hover:bg-surface-3 hover:text-fg-2 disabled:opacity-50"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="flex-1 space-y-3 overflow-y-auto px-5 py-4">
          {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

          {freeTables.length === 0 ? (
            <p className="rounded-lg border border-dashed border-line-strong px-4 py-8 text-center text-sm text-fg-muted">
              No free tables to shift to right now.
            </p>
          ) : (
            <ul className="divide-y divide-line">
              {freeTables.map((table) => (
                <li key={table.id}>
                  <button
                    type="button"
                    onClick={() => setSelectedId(table.id)}
                    disabled={isSubmitting}
                    className={`flex w-full items-center justify-between gap-3 rounded-xl px-2 py-2.5 text-left text-sm hover:bg-surface-2 ${
                      selectedId === table.id ? "bg-accent-soft" : ""
                    }`}
                  >
                    <span>
                      <span className="block font-medium text-fg">{table.name}</span>
                      <span className="block text-xs text-fg-muted">
                        {table.seats} seat{table.seats === 1 ? "" : "s"}
                      </span>
                    </span>
                    {selectedId === table.id && (
                      <span className="shrink-0 text-xs font-semibold text-accent-text">Selected</span>
                    )}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="space-y-2 border-t border-line p-5 pt-3">
          <button
            type="button"
            onClick={() => void confirm()}
            disabled={selectedId === null || isSubmitting}
            className="w-full rounded-xl bg-accent px-4 py-2.5 text-sm font-semibold text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isSubmitting ? "Shifting…" : "Confirm Shift"}
          </button>
          <button
            type="button"
            onClick={onClose}
            disabled={isSubmitting}
            className="w-full rounded-xl px-4 py-2.5 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50"
          >
            Cancel
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
