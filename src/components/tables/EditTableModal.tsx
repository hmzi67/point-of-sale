import { useState, type FormEvent } from "react";
import { X } from "lucide-react";
import type { TableSummary } from "../../types";

interface EditTableModalProps {
  table: TableSummary;
  onSave: (name: string) => Promise<void>;
  onClose: () => void;
}

/** Renames a table — TablesPage is admin/owner-only (see
 * `roleCanAccessModule`), so no further role check is needed here, same as
 * `AddTableForm`. Name only: seats/status have their own controls already
 * (the add form, and the card's reserve/clear actions). */
export function EditTableModal({ table, onSave, onClose }: EditTableModalProps) {
  const [name, setName] = useState(table.name);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = name.trim();
    if (!trimmed || trimmed === table.name) return;
    setIsSaving(true);
    setError(null);
    try {
      await onSave(trimmed);
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
          <h3 className="text-base font-semibold text-fg">Rename table</h3>
          <button
            type="button"
            onClick={onClose}
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
            disabled={isSaving || !name.trim() || name.trim() === table.name}
            className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
          >
            {isSaving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
    </div>
  );
}
