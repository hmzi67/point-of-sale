import { useState, type FormEvent } from "react";

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
    if (!name.trim()) return;
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
      className="flex flex-wrap items-end gap-3 rounded-md border border-slate-200 bg-slate-50 p-3"
    >
      <div>
        <label className="block text-xs font-medium text-slate-500">Table name</label>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Table 7"
          autoFocus
          className="mt-1 rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
        />
      </div>
      <div>
        <label className="block text-xs font-medium text-slate-500">Seats</label>
        <input
          type="number"
          min={1}
          value={seats}
          onChange={(e) => setSeats(Math.max(1, Number(e.target.value) || 1))}
          className="mt-1 w-20 rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
        />
      </div>

      {error && <p className="text-xs text-red-600">{error}</p>}

      <div className="flex gap-2">
        <button
          type="submit"
          disabled={isSaving || !name.trim()}
          className="rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? "Adding…" : "Add table"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md border border-slate-300 px-3 py-1.5 text-sm font-medium text-slate-600 hover:bg-white"
        >
          Cancel
        </button>
      </div>
    </form>
  );
}
