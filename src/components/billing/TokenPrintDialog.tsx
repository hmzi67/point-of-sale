import { useEffect, useState } from "react";
import { Printer, RotateCcw, X } from "lucide-react";
import {
  getAdhocTokenGroups,
  getPendingTokenItems,
  getTokensForOrder,
  printAdhocTokens,
  printTokens,
  reprintToken,
} from "../../services/tokensService";
import { getCounters } from "../../services/countersService";
import { formatQty } from "../../utils/format";
import type { AdhocTokenLine, CounterPrintResult, Counter, PendingCounterGroup, TokenSummary } from "../../types";

/** Where a "Print Token" dialog gets its pending items from — a dine-in
 * table's parked order (with real delta tracking and a reprintable
 * history), or a Takeaway cart that has no `table_orders` row at all (no
 * history, so every print sends the full quantity — see `printAdhocTokens`'s
 * doc comment). */
export type TokenPrintSource =
  | { kind: "table"; tableOrderId: number; tableName: string }
  | { kind: "adhoc"; items: AdhocTokenLine[] };

interface TokenPrintDialogProps {
  source: TokenPrintSource;
  onClose: () => void;
}

/**
 * "Print Token" (KOT) dialog — separate from "Complete Sale"/"Save to
 * table". Lists each counter that has un-tokenized items with a
 * default-checked checkbox so the cashier can deselect any counter they
 * don't want to print right now; items whose item has no counter never
 * appear here at all. For a table-linked order this also shows tokens
 * already printed, with a reprint option — a Takeaway (`adhoc`) order has
 * no persisted order to look that history up from, so it skips that section
 * and instead removes a counter from the list locally right after printing
 * it, so the same items can't be immediately reprinted by accident.
 */
export function TokenPrintDialog({ source, onClose }: TokenPrintDialogProps) {
  const [pending, setPending] = useState<PendingCounterGroup[]>([]);
  const [printed, setPrinted] = useState<TokenSummary[]>([]);
  const [counters, setCounters] = useState<Counter[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [isLoading, setIsLoading] = useState(true);
  const [isPrinting, setIsPrinting] = useState(false);
  const [reprintingId, setReprintingId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<CounterPrintResult[] | null>(null);

  const load = async () => {
    setIsLoading(true);
    setError(null);
    try {
      if (source.kind === "table") {
        const [pendingGroups, tokens, activeCounters] = await Promise.all([
          getPendingTokenItems(source.tableOrderId),
          getTokensForOrder(source.tableOrderId),
          getCounters(),
        ]);
        setPending(pendingGroups);
        setPrinted(tokens);
        setCounters(activeCounters);
        setSelected(new Set(pendingGroups.map((g) => g.counterId)));
      } else {
        const [pendingGroups, activeCounters] = await Promise.all([
          getAdhocTokenGroups(source.items),
          getCounters(),
        ]);
        setPending(pendingGroups);
        setPrinted([]);
        setCounters(activeCounters);
        setSelected(new Set(pendingGroups.map((g) => g.counterId)));
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source.kind, source.kind === "table" ? source.tableOrderId : source.items]);

  const toggle = (counterId: number) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(counterId)) next.delete(counterId);
      else next.add(counterId);
      return next;
    });
  };

  const handlePrint = async () => {
    if (selected.size === 0) return;
    setIsPrinting(true);
    setError(null);
    setResults(null);
    try {
      if (source.kind === "table") {
        const outcome = await printTokens(source.tableOrderId, Array.from(selected));
        setResults(outcome);
        await load();
      } else {
        const outcome = await printAdhocTokens(source.items, Array.from(selected));
        setResults(outcome);
        // No persisted order to re-derive pending from — a repeat call
        // would just report the exact same full quantities again (see
        // `printAdhocTokens`'s doc comment). Drop whatever just printed
        // from the local list instead, so it can't be reprinted by
        // accident from this same dialog.
        const printedCounterIds = new Set(
          outcome.filter((r) => r.outcome.status === "printed").map((r) => r.counterId),
        );
        setPending((current) => current.filter((g) => !printedCounterIds.has(g.counterId)));
        setSelected((current) => new Set(Array.from(current).filter((id) => !printedCounterIds.has(id))));
      }
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setIsPrinting(false);
    }
  };

  // Same pattern as `ReceiptModal`: plain Enter is this dialog's "do the
  // obvious thing" — print whatever's currently selected, same as clicking
  // "Print selected" — and Escape closes it, same as the X. Registered in
  // the capture phase so both keys are handled and stopped here before the
  // billing-surface listener (`useFastBillingHotkeys`) underneath ever sees
  // them; unlike `ReceiptModal`'s modal, this one isn't reflected in
  // `BillingPage`'s `fastBillingEnabled` by state alone but by
  // `onTokenDialogOpenChange`, so capturing here is the belt to that
  // suspenders. Enter no-ops (falls through to nothing) while printing is
  // already in flight or there's nothing selected to print — same guard
  // `handlePrint` and the button's own `disabled` already apply, so Enter
  // can never do more than the button could. Only Enter/Escape are
  // intercepted — Space still activates whichever button is focused (e.g. a
  // "Reprint" button), so tabbing to one and pressing Space still reprints
  // that specific token instead of the whole selection.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onClose();
        return;
      }
      if (event.key !== "Enter") return;
      event.preventDefault();
      event.stopPropagation();
      if (isPrinting || pending.length === 0 || selected.size === 0) return;
      void handlePrint();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isPrinting, pending.length, selected.size, onClose]);

  const handleReprint = async (tokenId: number) => {
    setReprintingId(tokenId);
    setError(null);
    try {
      await reprintToken(tokenId);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setReprintingId(null);
    }
  };

  const title = source.kind === "table" ? `Print token — ${source.tableName}` : "Print token — Takeaway";

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40 p-4">
      <div className="w-full max-w-md rounded-lg bg-white shadow-xl">
        <div className="flex items-center justify-between border-b border-slate-200 px-5 py-4">
          <h3 className="text-base font-semibold text-slate-900">{title}</h3>
          <button
            type="button"
            onClick={onClose}
            className="rounded-md p-1 text-slate-400 hover:bg-slate-100 hover:text-slate-600"
            aria-label="Close"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="max-h-[70dvh] space-y-4 overflow-y-auto px-5 py-4">
          {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

          {results && (
            <ul className="space-y-1 rounded-md border border-slate-200 p-3 text-sm">
              {results.map((r) => (
                <li key={r.counterId} className="flex items-center justify-between">
                  <span className="font-medium text-slate-700">{r.counterName}</span>
                  {r.outcome.status === "printed" && (
                    <span className="text-emerald-600">Printed — token #{r.outcome.token.tokenNumber}</span>
                  )}
                  {r.outcome.status === "nothingPending" && (
                    <span className="text-slate-400">Nothing pending</span>
                  )}
                  {r.outcome.status === "failed" && <span className="text-red-600">{r.outcome.error}</span>}
                </li>
              ))}
            </ul>
          )}

          {isLoading ? (
            <p className="text-sm text-slate-500">Loading…</p>
          ) : pending.length === 0 && counters.length === 0 ? (
            <p className="rounded-md bg-amber-50 px-3 py-3 text-sm text-amber-800">
              No counters have been set up yet, so no item can ever have a token to print. Go to{" "}
              <span className="font-medium">Settings → Counters</span> to add one (e.g. "Kitchen", "Drinks"),
              then open each item's edit form in <span className="font-medium">Inventory</span> and assign it
              a counter.
            </p>
          ) : pending.length === 0 && printed.length === 0 && !results ? (
            <p className="rounded-md bg-amber-50 px-3 py-3 text-sm text-amber-800">
              None of the items in this order are assigned to a counter, so there's nothing to token. Open
              each item's edit form in <span className="font-medium">Inventory</span> and set its{" "}
              <span className="font-medium">Counter</span> field — an item left unset (e.g. roti) will keep
              never printing a token, on purpose.
            </p>
          ) : pending.length === 0 ? (
            <p className="rounded-md bg-slate-50 px-3 py-3 text-sm text-slate-600">
              All items already sent to counters.
            </p>
          ) : (
            <div className="space-y-3">
              {pending.map((group) => (
                <label
                  key={group.counterId}
                  className="flex cursor-pointer items-start gap-3 rounded-md border border-slate-200 p-3 hover:bg-slate-50"
                >
                  <input
                    type="checkbox"
                    checked={selected.has(group.counterId)}
                    onChange={() => toggle(group.counterId)}
                    className="mt-0.5 h-4 w-4 rounded border-slate-300 text-brand-600 focus:ring-brand-400"
                  />
                  <div className="flex-1">
                    <p className="text-sm font-medium text-slate-900">{group.counterName}</p>
                    <ul className="mt-1 space-y-0.5 text-xs text-slate-600">
                      {group.items.map((line) => (
                        <li key={line.itemId} className="flex justify-between">
                          <span>{line.itemName}</span>
                          <span className="font-medium">{formatQty(line.qty, line.unit)}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                </label>
              ))}
            </div>
          )}

          {source.kind === "table" && printed.length > 0 && (
            <div>
              <p className="text-xs font-medium text-slate-500">Already printed</p>
              <ul className="mt-1 divide-y divide-slate-100 rounded-md border border-slate-200">
                {printed.map((token) => (
                  <li key={token.id} className="flex items-center justify-between px-3 py-2 text-sm">
                    <span>
                      #{token.tokenNumber} · {token.counterName}
                    </span>
                    <button
                      type="button"
                      onClick={() => void handleReprint(token.id)}
                      disabled={reprintingId === token.id}
                      className="flex items-center gap-1 text-xs font-medium text-slate-500 hover:text-brand-600 disabled:opacity-50"
                    >
                      <RotateCcw className="h-3.5 w-3.5" />
                      Reprint
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {source.kind === "adhoc" && (
            <p className="text-xs text-slate-400">
              Takeaway tokens aren't tracked against a saved order — printing again for the same items sends a
              fresh copy, so only print once you're ready.
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-slate-200 px-5 py-4">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-sm font-medium text-slate-600 hover:bg-slate-100"
          >
            Close
          </button>
          {pending.length > 0 && (
            <button
              type="button"
              onClick={() => void handlePrint()}
              disabled={isPrinting || selected.size === 0}
              className="flex items-center gap-1.5 rounded-md bg-brand-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-50"
            >
              <Printer className="h-4 w-4" />
              {isPrinting ? "Printing…" : "Print selected"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
