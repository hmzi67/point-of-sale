import { useEffect, useState } from "react";
import { Lock, Pencil, Plus, RotateCcw, Store } from "lucide-react";
import {
  addPlatform,
  listPlatforms,
  setPlatformActive,
  updatePlatform,
} from "../../services/deliveryService";
import type { DeliveryPlatform } from "../../types";
import { runAction } from "../../utils/runAction";

/**
 * Management of 3rd-party delivery platforms (e.g. Foodpanda, Cheetay, Careem).
 * Only rendered when the Delivery module is enabled.
 */
export function DeliveryPlatformsSection() {
  const [platforms, setPlatforms] = useState<DeliveryPlatform[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [newName, setNewName] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editingName, setEditingName] = useState("");
  const [pendingId, setPendingId] = useState<number | null>(null);

  const load = async () => {
    await runAction(setIsLoading, setError, async () => {
      const list = await listPlatforms();
      setPlatforms(list);
      setError(null);
    });
  };

  useEffect(() => {
    void load();
  }, []);

  const submitNewPlatform = async () => {
    if (!newName.trim()) return;
    await runAction(setIsAdding, setError, async () => {
      await addPlatform(newName.trim());
      setNewName("");
      await load();
    });
  };

  const submitRename = async (id: number) => {
    if (!editingName.trim()) return;
    setPendingId(id);
    try {
      await updatePlatform(id, editingName.trim());
      setEditingId(null);
      await load();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setPendingId(null);
    }
  };

  const toggleActive = async (platform: DeliveryPlatform) => {
    setPendingId(platform.id);
    try {
      await setPlatformActive(platform.id, !platform.isActive);
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
        <div className="flex items-center gap-2.5">
          <Store className="h-5 w-5 text-accent-text" />
          <h2 className="text-base font-semibold tracking-tight text-fg">Delivery Platforms</h2>
        </div>
        <p className="mt-1 text-sm text-fg-2">
          Third-party delivery partners (e.g. Foodpanda, Yango, Careem Now). Orders taken through these
          platforms will be recorded under their platform name and reconciled separately from drawer cash.
        </p>
      </div>

      {error && (
        <p className="border-b border-danger-line bg-danger-soft px-6 py-3 text-sm text-danger-text">
          {error}
        </p>
      )}

      <div className="flex gap-2 border-b border-line px-6 py-4">
        <input
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void submitNewPlatform();
            }
          }}
          placeholder="New platform name (e.g. Foodpanda)"
          className="w-full max-w-xs rounded-xl border border-line-strong px-3 py-2 text-sm"
        />
        <button
          type="button"
          onClick={() => void submitNewPlatform()}
          disabled={isAdding || !newName.trim()}
          className="flex shrink-0 items-center gap-1 rounded-xl bg-accent px-3 py-2 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:opacity-50"
        >
          <Plus className="h-4 w-4" />
          Add Platform
        </button>
      </div>

      {isLoading ? (
        <p className="px-6 py-4 text-sm text-fg-muted">Loading…</p>
      ) : platforms.length === 0 ? (
        <p className="px-6 py-4 text-sm text-fg-muted">
          No delivery platforms yet — add one above to take third-party delivery orders.
        </p>
      ) : (
        <ul className="divide-y divide-line">
          {platforms.map((platform) => (
            <li key={platform.id} className="flex items-center justify-between px-6 py-3">
              {editingId === platform.id ? (
                <div className="flex flex-1 gap-2">
                  <input
                    autoFocus
                    value={editingName}
                    onChange={(e) => setEditingName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void submitRename(platform.id);
                      }
                      if (e.key === "Escape") setEditingId(null);
                    }}
                    className="w-full max-w-xs rounded-xl border border-line-strong px-3 py-2 text-sm"
                  />
                  <button
                    type="button"
                    onClick={() => void submitRename(platform.id)}
                    disabled={pendingId === platform.id}
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
                    {platform.name}
                    {!platform.isActive && (
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
                        setEditingId(platform.id);
                        setEditingName(platform.name);
                      }}
                      className="flex items-center gap-1 text-xs font-medium text-fg-muted hover:text-accent-text"
                    >
                      <Pencil className="h-3.5 w-3.5" />
                      Rename
                    </button>
                    <button
                      type="button"
                      onClick={() => void toggleActive(platform)}
                      disabled={pendingId === platform.id}
                      className="flex items-center gap-1 text-xs font-medium text-fg-muted hover:text-danger-text disabled:opacity-50"
                    >
                      {platform.isActive ? (
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
