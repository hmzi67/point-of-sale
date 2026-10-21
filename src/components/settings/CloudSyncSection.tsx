import { useEffect, useState } from "react";
import { Cloud, CloudCheck, CloudAlert, RefreshCw, ExternalLink, CheckCircle2, AlertCircle } from "lucide-react";
import { getSyncStatus, syncNow, onSyncCompleted } from "../../services/syncService";
import type { SyncStatusInfo } from "../../types";

async function openExternal(url: string) {
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } catch {
    window.open(url, "_blank");
  }
}

export function CloudSyncSection() {
  const [status, setStatus] = useState<SyncStatusInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [isSyncing, setIsSyncing] = useState(false);
  const [notice, setNotice] = useState<{ tone: "ok" | "warn" | "error"; text: string } | null>(null);

  const fetchStatus = async () => {
    try {
      setLoading(true);
      const data = await getSyncStatus();
      setStatus(data);
    } catch (err) {
      setNotice({ tone: "error", text: `Couldn't load sync status: ${(err as Error).message}` });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void fetchStatus();

    const unsub = onSyncCompleted((result) => {
      void fetchStatus();
      if (result.success) {
        setNotice({
          tone: "ok",
          text: result.message || "Cloud records updated successfully.",
        });
      }
    });

    return () => {
      unsub.then((fn) => fn?.()).catch(() => {});
    };
  }, []);

  const handleSyncNow = async () => {
    setNotice(null);
    setIsSyncing(true);
    try {
      const res = await syncNow();
      await fetchStatus();
      if (res.success) {
        setNotice({
          tone: "ok",
          text: res.message || "Data successfully synchronized with the cloud server.",
        });
      } else {
        setNotice({
          tone: "warn",
          text: res.message || "Sync was unable to complete.",
        });
      }
    } catch (err) {
      setNotice({
        tone: "error",
        text: (err as Error).message || "An unexpected error occurred during sync.",
      });
    } finally {
      setIsSyncing(false);
    }
  };

  const portalUrl = "https://diwan.codehunts.co.uk/dashboard.html#records";

  return (
    <div id="cloud-sync" className="scroll-mt-4 rounded-2xl border border-line bg-surface p-6 shadow-soft">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-fg">
            <Cloud className="h-4 w-4 text-accent-text" />
            Cloud Database & Dashboard Sync
          </h2>
          <p className="mt-1 text-sm text-fg-2">
            Synchronizes your offline sales, daily turnover, and performance metrics with your online account
            at <code className="text-xs font-mono text-accent-text">diwan.codehunts.co.uk</code>.
          </p>
        </div>

        <button
          type="button"
          onClick={handleSyncNow}
          disabled={isSyncing || loading}
          className="inline-flex shrink-0 items-center justify-center gap-2 rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-accent-ink shadow-soft hover:bg-accent-hover disabled:opacity-50 transition"
        >
          <RefreshCw className={`h-4 w-4 ${isSyncing ? "animate-spin" : ""}`} />
          {isSyncing ? "Syncing..." : "Sync Now"}
        </button>
      </div>

      {notice && (
        <div
          className={`mt-4 flex items-start gap-2.5 rounded-xl border p-3 text-sm ${
            notice.tone === "ok"
              ? "border-success/30 bg-success-soft text-success-text"
              : notice.tone === "warn"
                ? "border-warning/30 bg-warning-soft text-warning-text"
                : "border-danger/30 bg-danger-soft text-danger-text"
          }`}
        >
          {notice.tone === "ok" ? (
            <CheckCircle2 className="h-4 w-4 shrink-0 mt-0.5" />
          ) : (
            <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
          )}
          <span>{notice.text}</span>
        </div>
      )}

      <dl className="mt-5 grid grid-cols-1 gap-4 text-sm sm:grid-cols-2 lg:grid-cols-3">
        <div className="rounded-xl border border-line/70 bg-surface-2 p-3.5">
          <dt className="text-xs font-medium text-fg-muted">Cloud Account</dt>
          <dd className="mt-1 font-semibold text-fg">
            {status?.serverCustomerId ? (
              <span className="inline-flex items-center gap-1.5 text-accent-text">
                <CloudCheck className="h-4 w-4" />
                Customer #{status.serverCustomerId}
              </span>
            ) : status?.isLicenseActive ? (
              <span className="text-fg-muted">Linked via Licence ({status.licensePrefix}...)</span>
            ) : (
              <span className="text-danger-text inline-flex items-center gap-1">
                <CloudAlert className="h-3.5 w-3.5" /> Not Activated
              </span>
            )}
          </dd>
        </div>

        <div className="rounded-xl border border-line/70 bg-surface-2 p-3.5">
          <dt className="text-xs font-medium text-fg-muted">Unsynced Local Sales</dt>
          <dd className="mt-1 font-semibold">
            {status ? (
              status.unsyncedCount === 0 ? (
                <span className="inline-flex items-center gap-1.5 text-success-text">
                  <span className="h-2 w-2 rounded-full bg-success"></span>
                  All sales uploaded (0 pending)
                </span>
              ) : (
                <span className="inline-flex items-center gap-1.5 text-warning-text">
                  <span className="h-2 w-2 rounded-full bg-warning animate-pulse"></span>
                  {status.unsyncedCount} {status.unsyncedCount === 1 ? "sale" : "sales"} waiting to sync
                </span>
              )
            ) : (
              <span className="text-fg-muted">Checking…</span>
            )}
          </dd>
        </div>

        <div className="rounded-xl border border-line/70 bg-surface-2 p-3.5">
          <dt className="text-xs font-medium text-fg-muted">Last Cloud Sync</dt>
          <dd className="mt-1 font-semibold text-fg">
            {status?.lastSyncedAt ? (
              new Date(status.lastSyncedAt).toLocaleString(undefined, {
                month: "short",
                day: "numeric",
                hour: "numeric",
                minute: "2-digit",
              })
            ) : (
              <span className="text-fg-muted">Pending initial sync</span>
            )}
          </dd>
        </div>
      </dl>

      <div className="mt-5 flex flex-wrap items-center justify-between gap-3 border-t border-line/70 pt-4 text-xs text-fg-muted">
        <div>
          <span>Target Server: </span>
          <span className="font-mono text-fg-2">https://diwan.codehunts.co.uk</span>
        </div>

        <button
          type="button"
          onClick={() => void openExternal(portalUrl)}
          className="inline-flex items-center gap-1 font-medium text-accent-text hover:underline"
        >
          <span>View Live Dashboard Online</span>
          <ExternalLink className="h-3.5 w-3.5" />
        </button>
      </div>
    </div>
  );
}
