import { useEffect, useState } from "react";
import { Cloud, CloudCheck, RefreshCw } from "lucide-react";
import { getSyncStatus, syncNow, onSyncCompleted } from "../../services/syncService";
import type { SyncStatusInfo } from "../../types";

export function CloudSyncBadge() {
  const [status, setStatus] = useState<SyncStatusInfo | null>(null);
  const [syncing, setSyncing] = useState(false);

  const refresh = async () => {
    try {
      const data = await getSyncStatus();
      setStatus(data);
    } catch {
      // offline or not ready
    }
  };

  useEffect(() => {
    void refresh();

    const unsub = onSyncCompleted(() => {
      void refresh();
    });

    const interval = setInterval(() => {
      void refresh();
    }, 60_000);

    return () => {
      clearInterval(interval);
      unsub.then((fn) => fn?.()).catch(() => {});
    };
  }, []);

  const handleSyncClick = async () => {
    if (syncing) return;
    setSyncing(true);
    try {
      await syncNow();
      await refresh();
    } catch {
      // ignore
    } finally {
      setSyncing(false);
    }
  };

  if (!status) return null;

  const isUpToDate = status.unsyncedCount === 0;

  return (
    <button
      type="button"
      onClick={handleSyncClick}
      title={
        syncing
          ? "Syncing data to diwan.codehunts.co.uk..."
          : isUpToDate
            ? `Cloud records up to date. Customer #${status.serverCustomerId ?? "linked"}. Click to sync now.`
            : `${status.unsyncedCount} sales pending cloud sync. Click to sync now.`
      }
      className={`flex items-center gap-1.5 rounded-xl border px-2.5 py-1.5 text-xs font-semibold shadow-soft transition ${
        syncing
          ? "border-accent/40 bg-accent-soft text-accent-text"
          : isUpToDate
            ? "border-line bg-surface text-fg-2 hover:border-line-hover hover:bg-surface-2"
            : "border-warning/50 bg-warning-soft text-warning-text hover:bg-warning-soft/80"
      }`}
    >
      {syncing ? (
        <RefreshCw className="h-3.5 w-3.5 animate-spin text-accent-text" />
      ) : isUpToDate ? (
        <CloudCheck className="h-3.5 w-3.5 text-success" />
      ) : (
        <Cloud className="h-3.5 w-3.5 text-warning" />
      )}
      <span className="hidden xl:inline">
        {syncing
          ? "Syncing…"
          : isUpToDate
            ? "Cloud Synced"
            : `Sync (${status.unsyncedCount})`}
      </span>
      {!isUpToDate && !syncing && (
        <span className="h-1.5 w-1.5 rounded-full bg-warning animate-pulse xl:hidden" />
      )}
    </button>
  );
}
