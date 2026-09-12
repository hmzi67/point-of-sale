import { useState, type FormEvent } from "react";
import { X } from "lucide-react";
import type { TableSummary } from "../../types";

import { SeatCountField } from "./SeatCountField";

interface EditTableModalProps {
  table: TableSummary;
  onSave: (name: string, seats: number) => Promise<void>;
  onClose: () => void;
}

export function EditTableModal({ table, onSave, onClose }: EditTableModalProps) {
  const [name, setName] = useState(table.name);
  const [seats, setSeats] = useState(table.seats);
  const unchanged = name.trim() === table.name && seats === table.seats;
  const validSeats = Number.isSafeInteger(seats) && seats > 0;
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed || unchanged || !validSeats) return;
    setIsSaving(true);
    setError(null);
    try {
      await onSave(trimmed, seats);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-4">
      <form
        onSubmit={(e) => void submit(e)}
        className="w-full max-w-sm rounded-lg bg-surface p-6 shadow-xl"
      >
        <div className="flex items-center justify-between">
          <h3 className="text-base font-semibold text-fg">Edit table</h3>
          <button
            type="button"
            onClick={onClose}
            disabled={isSaving}
            aria-label="Close edit table"
            className="rounded-full p-1 text-fg-muted hover:bg-surface-3 hover:text-fg-2"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <label className="mt-4 block">
          <span className="mb-1 block text-xs font-medium text-fg-muted">Table name</span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            autoFocus
            className="w-full rounded-md border border-line-strong px-2.5 py-1.5 text-sm focus:border-accent focus:outline-none"
          />
        </label>

        <div className="mt-4">
          <SeatCountField value={seats} onChange={setSeats} disabled={isSaving} />
        </div>

        {error && <p className="mt-2 text-xs text-danger-text">{error}</p>}

        <div className="mt-6 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={isSaving}
            className="rounded-md px-3 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface-3 disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={isSaving || !name.trim() || unchanged || !validSeats}
            className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isSaving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
    </div>
  );
}
