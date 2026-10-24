import { RefreshCw } from "lucide-react";
import { useWorkspaceStore } from "../../store";
import type { WorkspaceInfo } from "../../types";

/** Short words for the bar; the tooltip carries the detail. */
function describe(info: WorkspaceInfo): { dot: string; label: string } {
  switch (info.state) {
    case "synced":
      return { dot: "bg-success", label: "Saved centrally" };
    case "pending":
      return { dot: "bg-warning", label: `${info.pending} waiting to sync` };
    case "offline":
      return { dot: "bg-warning", label: info.pending > 0 ? `Offline · ${info.pending} waiting` : "Offline" };
    case "error":
      return { dot: "bg-danger", label: "Sync problem" };
    case "seeding":
      return { dot: "bg-accent animate-pulse", label: "Uploading business data…" };
    default:
      return { dot: "bg-warning", label: "Not connected yet" };
  }
}

/**
 * Where this counter's work is: on this device only ("Offline ready", not
 * connected to a business), saved centrally, or waiting to sync. Clicking
 * syncs now. Presentation of `workspace::runtime::WorkspaceInfo`.
 */
export function WorkspaceStatusChip() {
  const info = useWorkspaceStore((state) => state.info);
  const isSyncing = useWorkspaceStore((state) => state.isSyncing);
  const syncNow = useWorkspaceStore((state) => state.syncNow);

  if (!info || info.mode === "standalone") {
    return (
      <span className="hidden items-center gap-1.5 text-fg-muted sm:inline-flex" title="This counter is not connected to a business: its data is on this device only.">
        <span className="h-2 w-2 rounded-full bg-success" />
        Offline ready
      </span>
    );
  }

  const { dot, label } = describe(info);
  const detail = [
    info.businessName,
    info.lastOkAt ? `Last synced ${info.lastOkAt}` : "Not synced yet",
    info.lastError,
    info.parked > 0 ? `${info.parked} record(s) from other counters waiting — see Settings → Business & counters` : null,
  ]
    .filter(Boolean)
    .join("\n");

  return (
    <button
      type="button"
      onClick={() => void syncNow().catch(() => {})}
      disabled={isSyncing}
      title={`${detail}\nClick to sync now.`}
      className="hidden min-h-11 items-center gap-1.5 rounded-xl px-2 text-fg-muted hover:bg-surface-3 hover:text-fg disabled:opacity-70 sm:inline-flex"
    >
      {isSyncing ? <RefreshCw className="h-3.5 w-3.5 animate-spin" /> : <span className={`h-2 w-2 rounded-full ${dot}`} />}
      {info.counterName && <span className="font-medium text-fg-2">{info.counterName}</span>}
      <span>· {label}</span>
    </button>
  );
}
