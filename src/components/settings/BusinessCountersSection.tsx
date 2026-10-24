import { useEffect, useState } from "react";
import { AlertCircle, CheckCircle2, Building2, Monitor, RefreshCw, Unplug, X } from "lucide-react";
import {
  connectWorkspace,
  disconnectWorkspace,
  dismissWorkspaceIssue,
  downloadFromWorkspace,
  renameCounter,
  uploadToWorkspace,
} from "../../services/workspaceService";
import { useAppStore, useLicenseStore, useWorkspaceStore } from "../../store";
import type { WorkspaceConnectOutcome, WorkspaceInfo } from "../../types";

type Notice = { tone: "ok" | "warn" | "error"; text: string };

const STATE_LABEL: Record<WorkspaceInfo["state"], string> = {
  synced: "Everything is saved centrally",
  pending: "Changes waiting to sync",
  offline: "Offline — changes are kept here and sync when back online",
  error: "Sync problem",
  seeding: "Uploading this counter's data as the business data",
  standalone: "Not connected",
  setup: "Connecting",
};

const primaryBtn =
  "inline-flex items-center justify-center gap-2 rounded-xl bg-accent px-4 py-2 text-sm font-semibold text-accent-ink shadow-soft hover:bg-accent-hover disabled:opacity-50";
const ghostBtn =
  "inline-flex items-center justify-center gap-2 rounded-xl border border-line-strong bg-surface px-4 py-2 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:opacity-50";
const dangerBtn =
  "inline-flex items-center justify-center gap-2 rounded-xl bg-danger px-4 py-2 text-sm font-semibold text-white hover:opacity-90 disabled:opacity-50";

/**
 * Settings → Business & counters. One restaurant is one business: every
 * counter connected here shares its products, stock, sales, customers,
 * staff and settings (see `src-tauri/src/workspace/`). Owner/Admin only, like
 * the rest of Settings; every step is re-checked in Rust and on the server.
 *
 * Connecting is two steps on purpose: the server first says whether the
 * business already has data, and only then does the person choose — start
 * the business with this counter's data (empty business), or replace this
 * counter's data with the business's (backed up first). Nothing is ever
 * merged silently.
 */
export function BusinessCountersSection() {
  const info = useWorkspaceStore((state) => state.info);
  const refresh = useWorkspaceStore((state) => state.refresh);
  const setInfo = useWorkspaceStore((state) => state.setInfo);
  const syncNow = useWorkspaceStore((state) => state.syncNow);
  const isSyncing = useWorkspaceStore((state) => state.isSyncing);
  const licenseOk = useLicenseStore((state) => state.state?.kind === "ok");
  const offlineSales = useAppStore((state) => state.config.offlineSales);
  const saveConfig = useAppStore((state) => state.save);

  const [counterName, setCounterName] = useState("");
  const [outcome, setOutcome] = useState<WorkspaceConnectOutcome | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirmDisconnect, setConfirmDisconnect] = useState(false);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const run = async (step: string, action: () => Promise<void>) => {
    setBusy(step);
    setNotice(null);
    try {
      await action();
    } catch (err) {
      setNotice({ tone: "error", text: (err as Error).message });
    } finally {
      setBusy(null);
    }
  };

  const connect = () =>
    run("connect", async () => {
      const result = await connectWorkspace(counterName.trim() || null);
      setOutcome(result.next === "connected" ? null : result);
      await refresh();
      if (result.next === "connected") setNotice({ tone: "ok", text: `Reconnected to ${result.businessName}.` });
    });

  const upload = () =>
    run("upload", async () => {
      setInfo(await uploadToWorkspace());
      setOutcome(null);
      setNotice({
        tone: "ok",
        text: "This counter's data is being uploaded as the business data. Other counters can connect as soon as the top bar shows it is saved centrally.",
      });
    });

  const download = () =>
    run("download", async () => {
      const backup = await downloadFromWorkspace();
      setOutcome(null);
      // Rust signs everyone out next (`workspace:replaced`); this is shown
      // to whoever opens Settings again.
      setNotice({ tone: "ok", text: `This counter now uses the business data. Your previous data was saved to ${backup}.` });
    });

  const disconnect = () =>
    run("disconnect", async () => {
      await disconnectWorkspace();
      setConfirmDisconnect(false);
      await refresh();
      setNotice({ tone: "warn", text: "This counter no longer syncs. Its data stays on this device." });
    });

  const saveName = () =>
    run("rename", async () => {
      if (renaming === null) return;
      setInfo(await renameCounter(renaming));
      setRenaming(null);
    });

  const dismiss = (id: number) => run("dismiss", async () => setInfo(await dismissWorkspaceIssue(id)));

  const setOfflineSales = (value: "allow" | "block") =>
    run("offline", async () => {
      await saveConfig({ offlineSales: value });
      setNotice({
        tone: "ok",
        text: value === "block" ? "Counters now stop selling while they can't reach the server." : "Counters keep selling while offline.",
      });
    });

  return (
    <div id="business" className="scroll-mt-4 rounded-2xl border border-line bg-surface p-6 shadow-soft">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h2 className="flex items-center gap-2 text-base font-semibold tracking-tight text-fg">
            <Building2 className="h-4 w-4 text-accent-text" />
            Business &amp; counters
          </h2>
          <p className="mt-1 max-w-2xl text-sm text-fg-2">
            Connect every counter of your restaurant to one business: they all work on the same products and stock,
            see every counter's sales, and share staff accounts and settings. Each counter keeps working offline and
            catches up when it is back online.
          </p>
        </div>
        {info?.mode === "connected" && (
          <button type="button" className={primaryBtn} disabled={isSyncing} onClick={() => void run("sync", syncNow)}>
            <RefreshCw className={`h-4 w-4 ${isSyncing ? "animate-spin" : ""}`} />
            {isSyncing ? "Syncing…" : "Sync now"}
          </button>
        )}
      </div>

      {notice && <NoticeBox notice={notice} />}

      {!info && <p className="mt-4 text-sm text-fg-muted">Loading…</p>}

      {info && info.mode !== "connected" && !outcome && (
        <div className="mt-5 space-y-3">
          {!licenseOk && (
            <p className="rounded-xl border border-warning/30 bg-warning-soft p-3 text-sm text-warning-text">
              Activate the licence above first: a counter joins the business its licence belongs to, and uses one of
              the licence's device slots.
            </p>
          )}
          <div className="flex flex-col gap-2 sm:flex-row sm:items-end">
            <label className="flex-1 text-sm">
              <span className="font-medium text-fg-2">Name for this counter</span>
              <input
                className="mt-1 w-full rounded-xl border border-line bg-surface-2 px-3 py-2 text-sm text-fg"
                value={counterName}
                maxLength={60}
                placeholder="e.g. Front counter, Takeaway"
                onChange={(e) => setCounterName(e.target.value)}
              />
            </label>
            <button type="button" className={primaryBtn} disabled={!licenseOk || busy !== null} onClick={() => void connect()}>
              {busy === "connect" ? <RefreshCw className="h-4 w-4 animate-spin" /> : <Monitor className="h-4 w-4" />}
              {info.mode === "pending" ? "Continue connecting" : "Connect this counter"}
            </button>
          </div>
        </div>
      )}

      {outcome?.next === "upload" && (
        <ChoiceBox title={`${outcome.businessName} has no data yet`}>
          <p>
            Start it with this counter's data: its products and stock, sales history, customers, staff accounts and
            settings become the business's, and every counter you connect afterwards gets them.
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button type="button" className={primaryBtn} disabled={busy !== null} onClick={() => void upload()}>
              {busy === "upload" && <RefreshCw className="h-4 w-4 animate-spin" />}
              Use this counter's data for the business
            </button>
            <button type="button" className={ghostBtn} disabled={busy !== null} onClick={() => setOutcome(null)}>
              Cancel
            </button>
          </div>
        </ChoiceBox>
      )}

      {outcome?.next === "download" && (
        <ChoiceBox title={`${outcome.businessName} already has its data`} danger>
          <p>
            This counter will use the business's products, stock, sales, staff and settings
            {outcome.localRecords > 0 ? (
              <>
                {" "}— its own {outcome.localRecords.toLocaleString()} current records are <strong>replaced</strong>, after a
                backup copy of this counter's database is saved on this device
              </>
            ) : null}
            . Everyone is signed out afterwards; sign in with a business staff account.
          </p>
          <p className="mt-1 text-fg-muted">
            {outcome.counters} of {outcome.maxCounters} counters allowed by the licence{outcome.plan ? ` (${outcome.plan})` : ""}.
          </p>
          <div className="mt-3 flex flex-wrap gap-2">
            <button type="button" className={dangerBtn} disabled={busy !== null} onClick={() => void download()}>
              {busy === "download" && <RefreshCw className="h-4 w-4 animate-spin" />}
              {busy === "download" ? "Loading the business data…" : "Replace with the business data"}
            </button>
            <button type="button" className={ghostBtn} disabled={busy !== null} onClick={() => setOutcome(null)}>
              Cancel
            </button>
          </div>
        </ChoiceBox>
      )}

      {outcome?.next === "wait" && (
        <ChoiceBox title="Another counter is still uploading the business data">
          <p>Try again in a minute, once that counter shows its data is saved centrally.</p>
          <div className="mt-3">
            <button type="button" className={ghostBtn} disabled={busy !== null} onClick={() => void connect()}>
              Check again
            </button>
          </div>
        </ChoiceBox>
      )}

      {info?.mode === "connected" && (
        <>
          <dl className="mt-5 grid grid-cols-1 gap-4 text-sm sm:grid-cols-2 lg:grid-cols-4">
            <Fact label="Business">{info.businessName ?? "—"}</Fact>
            <Fact label="This counter">
              {renaming === null ? (
                <span className="inline-flex items-center gap-2">
                  {info.counterName}
                  {info.counterCode && <code className="text-xs text-fg-muted">{info.counterCode}</code>}
                  <button type="button" className="text-xs font-medium text-accent-text hover:underline" onClick={() => setRenaming(info.counterName ?? "")}>
                    Rename
                  </button>
                </span>
              ) : (
                <span className="flex gap-2">
                  <input
                    className="w-full rounded-lg border border-line bg-surface px-2 py-1 text-sm text-fg"
                    value={renaming}
                    maxLength={60}
                    autoFocus
                    onChange={(e) => setRenaming(e.target.value)}
                  />
                  <button type="button" className="text-xs font-semibold text-accent-text" disabled={busy !== null} onClick={() => void saveName()}>
                    Save
                  </button>
                </span>
              )}
            </Fact>
            <Fact label="Status">
              <span className={info.state === "synced" ? "text-success-text" : info.state === "error" ? "text-danger-text" : "text-warning-text"}>
                {STATE_LABEL[info.state]}
              </span>
              {info.lastError && <span className="mt-1 block text-xs font-normal text-fg-muted">{info.lastError}</span>}
            </Fact>
            <Fact label="Waiting to sync">
              {info.pending === 0 ? "Nothing" : `${info.pending} change${info.pending === 1 ? "" : "s"}`}
              <span className="mt-1 block text-xs font-normal text-fg-muted">
                {info.lastOkAt ? `Last synced ${info.lastOkAt}` : "Not synced yet"}
              </span>
            </Fact>
          </dl>

          <h3 className="mt-6 text-sm font-semibold text-fg">Counters in this business</h3>
          <ul className="mt-2 divide-y divide-line rounded-xl border border-line">
            {info.counters.length === 0 && <li className="px-4 py-3 text-sm text-fg-muted">Listed after the next sync.</li>}
            {info.counters.map((counter) => (
              <li key={counter.code} className="flex items-center justify-between gap-3 px-4 py-2.5 text-sm">
                <span className="flex items-center gap-2 text-fg">
                  <Monitor className="h-4 w-4 text-fg-muted" />
                  {counter.name}
                  {counter.thisCounter && <span className="rounded bg-accent-soft px-1.5 py-0.5 text-xs text-accent-text">this counter</span>}
                  {!counter.active && <span className="rounded bg-surface-3 px-1.5 py-0.5 text-xs text-fg-muted">disconnected</span>}
                </span>
                <span className="text-xs text-fg-muted">{counter.lastSeenAt ? `Seen ${counter.lastSeenAt}` : "Not seen yet"}</span>
              </li>
            ))}
          </ul>

          {info.issues.length > 0 && (
            <>
              <h3 className="mt-6 text-sm font-semibold text-fg">Needs a look</h3>
              <ul className="mt-2 space-y-2">
                {info.issues.map((issue, index) => (
                  <li key={`${issue.id}-${index}`} className="flex items-start justify-between gap-3 rounded-xl border border-warning/30 bg-warning-soft p-3 text-sm text-warning-text">
                    <span>
                      {issue.detail}
                      {issue.kind === "waiting" && (
                        <span className="mt-1 block text-xs">
                          Usually a name or barcode that two counters created at the same time — rename one and it applies on the next sync.
                        </span>
                      )}
                    </span>
                    {issue.id > 0 && (
                      <button type="button" aria-label="Dismiss" className="shrink-0" onClick={() => void dismiss(issue.id)}>
                        <X className="h-4 w-4" />
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}

          <label className="mt-6 block text-sm">
            <span className="font-semibold text-fg">When a counter can't reach the server</span>
            <select
              className="mt-1 block w-full max-w-md rounded-xl border border-line bg-surface-2 px-3 py-2 pr-8 text-sm text-fg"
              value={offlineSales}
              disabled={busy !== null}
              onChange={(e) => void setOfflineSales(e.target.value as "allow" | "block")}
            >
              <option value="allow">Keep selling (stock is checked when it reconnects)</option>
              <option value="block">Stop selling until it reconnects</option>
            </select>
            <span className="mt-1 block max-w-2xl text-xs text-fg-muted">
              Offline, a counter can't check the other counters' sales, so it may sell stock that is already gone — that is
              flagged afterwards. Stopping instead means no sale at all while the internet is down. Applies to every counter.
            </span>
          </label>

          <div className="mt-6 flex flex-wrap items-center justify-between gap-3 border-t border-line/70 pt-4 text-xs text-fg-muted">
            <span>
              Sales take their stock from the business's central stock before they are recorded, so two counters never sell
              the last unit twice. Product photos and the logo are shared too.
            </span>
            {confirmDisconnect ? (
              <span className="flex items-center gap-2">
                <span className="text-fg-2">Stop syncing this counter?</span>
                <button type="button" className={dangerBtn} disabled={busy !== null} onClick={() => void disconnect()}>
                  Disconnect
                </button>
                <button type="button" className={ghostBtn} onClick={() => setConfirmDisconnect(false)}>
                  Keep
                </button>
              </span>
            ) : (
              <button type="button" className="inline-flex items-center gap-1 font-medium text-fg-2 hover:text-danger-text" onClick={() => setConfirmDisconnect(true)}>
                <Unplug className="h-3.5 w-3.5" />
                Disconnect this counter
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}

function NoticeBox({ notice }: { notice: Notice }) {
  const tone =
    notice.tone === "ok"
      ? "border-success/30 bg-success-soft text-success-text"
      : notice.tone === "warn"
        ? "border-warning/30 bg-warning-soft text-warning-text"
        : "border-danger/30 bg-danger-soft text-danger-text";
  return (
    <div role="status" className={`mt-4 flex items-start gap-2.5 rounded-xl border p-3 text-sm ${tone}`}>
      {notice.tone === "ok" ? <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" /> : <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />}
      <span className="break-words">{notice.text}</span>
    </div>
  );
}

function ChoiceBox({ title, danger = false, children }: { title: string; danger?: boolean; children: React.ReactNode }) {
  return (
    <div className={`mt-5 rounded-xl border p-4 text-sm text-fg-2 ${danger ? "border-danger-line bg-danger-soft/40" : "border-line bg-surface-2"}`}>
      <h3 className="mb-1 font-semibold text-fg">{title}</h3>
      {children}
    </div>
  );
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="rounded-xl border border-line/70 bg-surface-2 p-3.5">
      <dt className="text-xs font-medium text-fg-muted">{label}</dt>
      <dd className="mt-1 font-semibold text-fg">{children}</dd>
    </div>
  );
}
