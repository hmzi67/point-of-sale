import { useState, type FormEvent } from "react";

import { SeatCountField } from "./SeatCountField";

interface AddTableFormProps {
  onAdd: (name: string, seats: number) => Promise<void>;
  onCancel: () => void;
}

/** Inline "add a table" form — TablesPage is admin/owner-only (see
 * `roleCanAccessModule`), so no further role check is needed here. */
export function AddTableForm({ onAdd, onCancel }: AddTableFormProps) {
  const [name, setName] = useState("");
  const [seats, setSeats] = useState(4);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !Number.isSafeInteger(seats) || seats < 1) return;
    setIsSaving(true);
    setError(null);
    try {
      await onAdd(name.trim(), seats);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <form
      onSubmit={(e) => void submit(e)}
      className="flex flex-wrap items-end gap-3 rounded-md border border-line bg-surface-2 p-3"
    >
      <div>
        <label className="block text-xs font-medium text-fg-muted">Table name</label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Table 7"
          autoFocus
          className="mt-1 rounded-md border border-line-strong px-2.5 py-1.5 text-sm"
        />
      </div>
      <SeatCountField value={seats} onChange={setSeats} disabled={isSaving} />

      {error && <p className="text-xs text-danger-text">{error}</p>}

      <div className="flex gap-2">
        <button
          type="submit"
          disabled={isSaving || !name.trim()}
          className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? "Adding…" : "Add table"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md border border-line-strong px-3 py-1.5 text-sm font-medium text-fg-2 hover:bg-surface"
        >
          Cancel
        </button>
      </div>
    </form>
  );
}
