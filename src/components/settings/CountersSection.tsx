import { useEffect, useState } from "react";
import { Lock, Pencil, Plus, RotateCcw } from "lucide-react";
import {
  addCounter,
  getCounters,
  setCounterActive,
  updateCounter,
} from "../../services/countersService";
import type { Counter } from "../../types";
import { runAction } from "../../utils/runAction";

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
    await runAction(setIsLoading, setError, async () => {
      const list = await getCounters(true);
      setCounters(list);
      setError(null);
    });
  };

  useEffect(() => {
    void load();
  }, []);

  const submitNewCounter = async () => {
    if (!newName.trim()) return;
    await runAction(setIsAdding, setError, async () => {
      await addCounter(newName.trim());
      setNewName("");
      await load();
    });
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
    <div className="rounded-2xl border border-line bg-surface">
      <div className="border-b border-line bg-surface-2/30 p-6">
        <h2 className="text-base font-semibold tracking-tight text-fg">Counters</h2>
        <p className="mt-1 text-sm text-fg-2">
          The kitchen/prep stations a KOT token can print to (e.g. "Channa Counter", "Drinks Counter",
          "Tandoor"). Assign items to a counter from the item's edit form — an item with no counter never
          prints a token.
        </p>
      </div>

      {error && <p className="border-b border-danger-line bg-danger-soft px-6 py-3 text-sm text-danger-text">{error}</p>}

      <div className="flex gap-2 border-b border-line px-6 py-4">
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
          className="w-full max-w-xs rounded-xl border border-line-strong px-3 py-2 text-sm"
        />
        <button
          type="button"
          onClick={() => void submitNewCounter()}
          disabled={isAdding || !newName.trim()}
          className="flex shrink-0 items-center gap-1 rounded-xl bg-accent px-3 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:opacity-50"
        >
          <Plus className="h-4 w-4" />
          Add
        </button>
      </div>

      {isLoading ? (
        <p className="px-6 py-4 text-sm text-fg-muted">Loading…</p>
      ) : counters.length === 0 ? (
        <p className="px-6 py-4 text-sm text-fg-muted">
          No counters yet — add one above before you can print tokens.
        </p>
      ) : (
        <ul className="divide-y divide-line">
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
                    className="w-full max-w-xs rounded-xl border border-line-strong px-3 py-2 text-sm"
                  />
                  <button
                    type="button"
                    onClick={() => void submitRename(counter.id)}
                    disabled={pendingId === counter.id}
                    className="rounded-xl bg-accent px-3 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:opacity-50"
                  >
                    Save
                  </button>
                  <button
                    type="button"
                    onClick={() => setEditingId(null)}
                    className="rounded-xl px-3 py-2 text-sm text-fg-muted hover:bg-surface-3"
                  >
                    Cancel
                  </button>
                </div>
              ) : (
                <>
                  <p className="flex items-center gap-2 text-sm font-medium text-fg">
                    {counter.name}
                    {!counter.isActive && (
                      <span className="inline-flex items-center gap-1 rounded bg-surface-3 px-1.5 py-0.5 text-xs font-normal text-fg-muted">
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
                      className="flex items-center gap-1 text-xs font-medium text-fg-muted hover:text-accent-text"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                      Rename
                    </button>
                    <button
                      type="button"
                      onClick={() => void toggleActive(counter)}
                      disabled={pendingId === counter.id}
                      className="flex items-center gap-1 text-xs font-medium text-fg-muted hover:text-danger-text disabled:opacity-50"
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
