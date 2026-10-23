import { useEffect, useRef, useState } from "react";
import { Cloud, CloudAlert, CloudCheck, RefreshCw } from "lucide-react";
import { reportWarning } from "../../services/logService";
import { getSyncStatus, syncNow, onSyncCompleted } from "../../services/syncService";
import type { SyncStatusInfo } from "../../types";

export function CloudSyncBadge() {
  const [status, setStatus] = useState<SyncStatusInfo | null>(null);
  const [syncing, setSyncing] = useState(false);
  /** Why the last manual sync failed; cleared by the next success. */
  const [failure, setFailure] = useState<string | null>(null);
  // The status poll runs every minute, so a lasting failure (offline) is
  // logged once per streak rather than once per poll.
  const statusFailing = useRef(false);

  const refresh = async () => {
    try {
      setStatus(await getSyncStatus());
      statusFailing.current = false;
    } catch (e) {
      if (!statusFailing.current) reportWarning(`sync badge: status unavailable: ${(e as Error).message}`);
      statusFailing.current = true;
    }
  };

  useEffect(() => {
    void refresh();

    let unlisten: (() => void) | null = null;
    let disposed = false;
    void onSyncCompleted(() => {
      setFailure(null);
      void refresh();
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });

    const interval = setInterval(() => {
      void refresh();
    }, 60_000);

    return () => {
      disposed = true;
      clearInterval(interval);
      unlisten?.();
    };
  }, []);

  const handleSyncClick = async () => {
    if (syncing) return;
    setSyncing(true);
    try {
      const result = await syncNow();
      setFailure(result.success ? null : result.message || "Sync was unable to complete.");
      await refresh();
    } catch (e) {
      setFailure((e as Error).message || "Sync failed.");
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
          : failure
            ? `Sync failed: ${failure}. Click to retry.`
            : isUpToDate
            ? `Cloud records up to date. Customer #${status.serverCustomerId ?? "linked"}. Click to sync now.`
            : `${status.unsyncedCount} sales pending cloud sync. Click to sync now.`
      }
      className={`flex items-center gap-1.5 rounded-xl border px-2.5 py-1.5 text-xs font-semibold shadow-soft transition ${
        syncing
          ? "border-accent/40 bg-accent-soft text-accent-text"
          : failure
            ? "border-danger-line bg-danger-soft text-danger-text hover:bg-danger-soft/80"
            : isUpToDate
            ? "border-line bg-surface text-fg-2 hover:border-line-hover hover:bg-surface-2"
            : "border-warning/50 bg-warning-soft text-warning-text hover:bg-warning-soft/80"
      }`}
    >
      {syncing ? (
        <RefreshCw className="h-3.5 w-3.5 animate-spin text-accent-text" />
      ) : failure ? (
        <CloudAlert className="h-3.5 w-3.5" />
      ) : isUpToDate ? (
        <CloudCheck className="h-3.5 w-3.5 text-success" />
      ) : (
        <Cloud className="h-3.5 w-3.5 text-warning" />
      )}
      <span className="hidden xl:inline">
        {syncing
          ? "Syncing…"
          : failure
            ? "Sync failed"
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
