import { useEffect, useState } from "react";
import { Lock, Pencil, Plus, RotateCcw } from "lucide-react";
import {
  addCounter,
  getCounters,
  setCounterActive,
  updateCounter,
} from "../../services/countersService";
import type { Counter } from "../../types";

/**
 * Owner/Admin management of kitchen/prep counters for the KOT token
 * workflow — see `src-tauri/src/db/counters.rs`. Deliberately separate from
 * inventory categories: a counter is a physical station, not a browse
 * grouping, and clients define their own since this varies per business.
 * Only rendered when the Tables module is enabled — see `SettingsPage`.
 */
export function CountersSection() {
  const [counters, setCounters] = useState<Counter[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [newName, setNewName] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingName, setEditingName] = useState("");
  const [pendingId, setPendingId] = useState<number | null>(null);

  const load = async () => {
    setIsLoading(true);
    try {
      const list = await getCounters(true);
      setCounters(list);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const submitNewCounter = async () => {
    if (!newName.trim()) return;
    setIsAdding(true);
    try {
      await addCounter(newName.trim());
      setNewName("");
      await load();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsAdding(false);
    }
  };

  const submitRename = async (id: number) => {
    if (!editingName.trim()) return;
    setPendingId(id);
    try {
      await updateCounter(id, editingName.trim());
      setEditingId(null);
      await load();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setPendingId(null);
    }
  };

  const toggleActive = async (counter: Counter) => {
    setPendingId(counter.id);
    try {
      await setCounterActive(counter.id, !counter.isActive);
      await load();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setPendingId(null);
    }
  };

  return (
    <div className="rounded-lg border border-slate-200 bg-white">
      <div className="border-b border-slate-200 p-6">
        <h2 className="text-lg font-semibold text-slate-900">Counters</h2>
        <p className="mt-1 text-sm text-slate-600">
          The kitchen/prep stations a KOT token can print to (e.g. "Channa Counter", "Drinks Counter",
          "Tandoor"). Assign items to a counter from the item's edit form — an item with no counter never
          prints a token.
        </p>
      </div>

      {error && <p className="border-b border-red-100 bg-red-50 px-6 py-3 text-sm text-red-700">{error}</p>}

      <div className="flex gap-2 border-b border-slate-100 px-6 py-4">
        <input
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void submitNewCounter();
            }
          }}
          placeholder="New counter name"
          className="w-full max-w-xs rounded-md border border-slate-300 px-3 py-2 text-sm"
        />
        <button
          type="button"
          onClick={() => void submitNewCounter()}
          disabled={isAdding || !newName.trim()}
          className="flex shrink-0 items-center gap-1 rounded-md bg-brand-600 px-3 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-50"
        >
          <Plus className="h-4 w-4" />
          Add
        </button>
      </div>

      {isLoading ? (
        <p className="px-6 py-4 text-sm text-slate-500">Loading…</p>
      ) : counters.length === 0 ? (
        <p className="px-6 py-4 text-sm text-slate-500">
          No counters yet — add one above before you can print tokens.
        </p>
      ) : (
        <ul className="divide-y divide-slate-100">
          {counters.map((counter) => (
            <li key={counter.id} className="flex items-center justify-between px-6 py-3">
              {editingId === counter.id ? (
                <div className="flex flex-1 gap-2">
                  <input
                    autoFocus
                    value={editingName}
                    onChange={(e) => setEditingName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void submitRename(counter.id);
                      }
                      if (e.key === "Escape") setEditingId(null);
                    }}
                    className="w-full max-w-xs rounded-md border border-slate-300 px-3 py-2 text-sm"
                  />
                  <button
                    type="button"
                    onClick={() => void submitRename(counter.id)}
                    disabled={pendingId === counter.id}
                    className="rounded-md bg-brand-600 px-3 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-50"
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    onClick={() => setEditingId(null)}
                    className="rounded-md px-3 py-2 text-sm text-slate-500 hover:bg-slate-100"
                  >
                    Cancel
                  </button>
                </div>
              ) : (
                <>
                  <p className="flex items-center gap-2 text-sm font-medium text-slate-900">
                    {counter.name}
                    {!counter.isActive && (
                      <span className="inline-flex items-center gap-1 rounded bg-slate-100 px-1.5 py-0.5 text-xs font-normal text-slate-500">
                        <Lock className="h-3 w-3" />
                        Deactivated
                      </span>
                    )}
                  </p>
                  <div className="flex items-center gap-3">
                    <button
                      type="button"
                      onClick={() => {
                        setEditingId(counter.id);
                        setEditingName(counter.name);
                      }}
                      className="flex items-center gap-1 text-xs font-medium text-slate-500 hover:text-brand-600"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                      Rename
                    </button>
                    <button
                      type="button"
                      onClick={() => void toggleActive(counter)}
                      disabled={pendingId === counter.id}
                      className="flex items-center gap-1 text-xs font-medium text-slate-500 hover:text-red-600 disabled:opacity-50"
                    >
                      {counter.isActive ? (
                        "Deactivate"
                      ) : (
                        <>
                          <RotateCcw className="h-3.5 w-3.5" />
                          Reactivate
                        </>
                      )}
                    </button>
                  </div>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
