import { useEffect, useRef, useState } from "react";
import { useBillingStore } from "../store";
import type { Item } from "../types";

/**
 * Keyboard-first fast billing for the desktop Billing screen (see
 * `BillingPage`). Not used on Android — that platform stays touch-first, and
 * `BillingPage` only mounts this hook's consumer when `!IS_ANDROID`.
 *
 * One `window`-level `keydown` listener drives everything:
 *  - digits build up a quick-entry buffer; Enter looks the buffer up against
 *    `barcode` first, then `shortCode` (same priority order the existing
 *    barcode-scan-to-add search already uses — see `ItemSearchBar`/`db::
 *    items::search_items`), and adds a match to the cart.
 *  - `T` (only when the buffer is empty and the Tables module is enabled)
 *    opens the table quick-select popup — the popup owns its own numeric
 *    input from there, so this hook only needs to open/close it.
 *  - Up/Down move the cart's "active line" selection; Left/Right adjust its
 *    qty; Delete/Backspace remove it — all against `useBillingStore`
 *    directly, this hook doesn't sit in the render path of the cart itself.
 *  - Enter with an empty buffer and a `soldByAmount` active line opens the
 *    amount-entry popup for it (see `ItemAmountEntryModal`) — the line got
 *    there at qty 0 from `billingStore.addItem`, and this is how a cashier
 *    types the actual rupee amount a customer asked for.
 *  - Ctrl+Enter (Cmd+Enter on macOS) places the order — calls the exact same
 *    `onPlaceOrder` the mouse "Place Order" button's `onClick` calls, so
 *    every condition/validation/error-banner path it already has (empty
 *    cart, a submission in flight, a server-side rejection, …) applies
 *    identically here with zero duplicated logic. Checked ahead of the
 *    plain-Enter buffer-lookup/amount-popup branches below and independent
 *    of the buffer's contents — it's a distinct combination, never treated
 *    as part of a typed item code.
 *  - Ctrl+K (Cmd+K on macOS) opens "Print Token" for the current order —
 *    mnemonic for KOT (Kitchen Order Ticket). Fires `onPrintToken` whenever
 *    the Tables module is enabled, dine-in (`tableId` set) or Takeaway
 *    (`tableId === null`) alike — `OrderTypeAndTable` (the component that
 *    owns the actual dialog) branches on `tableId` itself to open the right
 *    flow, the same way its mouse "Print token" button already does. With
 *    the Tables module off there's no `OrderTypeAndTable` mounted to react
 *    to this at all, so it's a silent no-op there, same as every other
 *    shortcut here that has no target. The "nothing to token" / "everything
 *    already tokenized" cases beyond that are left entirely to
 *    `onPrintToken`'s own flow (`openTableTokenDialog`/`openTakeawayTokenDialog`
 *    + `TokenPrintDialog`) — it already surfaces the same messages the mouse
 *    "Print token" button does, so there's nothing for this hook to
 *    duplicate.
 *  - `?` toggles a shortcuts help overlay.
 *
 * Deliberately does nothing while focus is on a genuine text input (search
 * bar, discount field, notes field, the table popup's own input, …) — a
 * scanner or a cashier typing into a real field must never be hijacked by
 * this. That's also what makes the table popup's own input "just work"
 * without any special-casing here: once it's focused, `isTextEntryFocused`
 * is true and this listener gets out of the way entirely.
 */
export function useFastBillingHotkeys(options: {
  enabled: boolean;
  items: Item[];
  tablesEnabled: boolean;
  /** The cart's current table, straight from `useBillingStore` — kept in the
   * hook's deps so a stale closure never reads it, though Ctrl+K itself no
   * longer branches on it directly (see the hook's doc comment). */
  tableId: number | null;
  /** Called on Ctrl/Cmd+Enter — the mouse "Place Order" button's own click
   * handler, passed straight through so this shortcut can never drift from
   * whatever conditions/validation that button already enforces. */
  onPlaceOrder: () => void;
  /** Called on Ctrl/Cmd+K whenever the Tables module is enabled — triggers
   * the same "Print Token" flow the mouse button opens, for a dine-in table
   * or a Takeaway order alike (see `OrderTypeAndTable`). */
  onPrintToken: () => void;
}) {
  const { enabled, items, tablesEnabled, tableId, onPlaceOrder, onPrintToken } = options;

  const [buffer, setBuffer] = useState("");
  const [tablePopupOpen, setTablePopupOpen] = useState(false);
  const [showHelp, setShowHelp] = useState(false);
  const bufferClearTimer = useRef<ReturnType<typeof window.setTimeout> | null>(null);

  const addItem = useBillingStore((state) => state.addItem);
  const setActiveLine = useBillingStore((state) => state.setActiveLine);
  const activeLineItemId = useBillingStore((state) => state.activeLineItemId);
  const moveActiveLine = useBillingStore((state) => state.moveActiveLine);
  const adjustActiveQty = useBillingStore((state) => state.adjustActiveQty);
  const removeActiveLine = useBillingStore((state) => state.removeActiveLine);
  const requestAmountEntry = useBillingStore((state) => state.requestAmountEntry);

  const clearBufferSoon = () => {
    if (bufferClearTimer.current !== null) window.clearTimeout(bufferClearTimer.current);
    // A stale buffer left on screen after the cashier walks away/gets
    // distracted reads as a bug ("why does it say '3_'?") — auto-clear after
    // a pause, same idea as a search box's debounce but the other direction.
    bufferClearTimer.current = window.setTimeout(() => setBuffer(""), 3000);
  };

  useEffect(() => () => {
    if (bufferClearTimer.current !== null) window.clearTimeout(bufferClearTimer.current);
  }, []);

  useEffect(() => {
    if (!enabled) return;

    function isTextEntryFocused(): boolean {
      const el = document.activeElement as HTMLElement | null;
      if (!el) return false;
      const tag = el.tagName;
      return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable;
    }

    function onKeyDown(event: KeyboardEvent) {
      // "?" toggles the help overlay from anywhere on the billing surface,
      // even while a buffer is mid-type — it's a rare enough key that a
      // short code would never legitimately contain it.
      if (event.key === "?" && !isTextEntryFocused()) {
        event.preventDefault();
        setShowHelp((v) => !v);
        return;
      }

      // While the help overlay is open, swallow everything except the keys
      // that close it — arrow/Delete/digit keys must not silently edit the
      // cart behind an overlay the cashier is busy reading.
      if (showHelp) {
        if (event.key === "Escape") setShowHelp(false);
        return;
      }

      if (isTextEntryFocused()) return;

      // Distinct from plain Enter (which confirms/looks-up whatever is in
      // the quick-entry buffer below) — checked first and independent of
      // `buffer`, so it fires the same way whether the cashier has typed a
      // code or not, and never gets swallowed into that buffer's own Enter
      // handling.
      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        onPlaceOrder();
        return;
      }

      // Ctrl/Cmd+K: "Print Token" (KOT). Distinct combination, independent
      // of `buffer`, same as Ctrl+Enter above. Fires for both a dine-in
      // table and a Takeaway order — `OrderTypeAndTable` picks the right
      // flow based on `tableId` itself. Only requires the Tables module to
      // be enabled, since that's what decides whether `OrderTypeAndTable`
      // (and its "Print Token" dialog) is even mounted to react to this.
      if ((event.key === "k" || event.key === "K") && (event.metaKey || event.ctrlKey)) {
        if (!tablesEnabled) return;
        event.preventDefault();
        onPrintToken();
        return;
      }

      if (event.key === "Escape") {
        setBuffer("");
        return;
      }

      if (event.key === "ArrowUp") {
        event.preventDefault();
        moveActiveLine(-1);
        return;
      }
      if (event.key === "ArrowDown") {
        event.preventDefault();
        moveActiveLine(1);
        return;
      }
      if (event.key === "ArrowRight") {
        event.preventDefault();
        adjustActiveQty(1);
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        adjustActiveQty(-1);
        return;
      }
      if (event.key === "Delete" || event.key === "Backspace") {
        event.preventDefault();
        removeActiveLine();
        return;
      }

      if ((event.key === "t" || event.key === "T") && buffer === "" && tablesEnabled && !tablePopupOpen) {
        event.preventDefault();
        setTablePopupOpen(true);
        return;
      }

      if (/^[0-9]$/.test(event.key)) {
        event.preventDefault();
        setBuffer((current) => (current + event.key).slice(0, 6));
        clearBufferSoon();
        return;
      }

      if (event.key === "Enter" && buffer !== "") {
        event.preventDefault();
        const code = buffer;
        setBuffer("");

        // Same priority order the existing scan-to-add search uses: an exact
        // barcode match first, short code only as a fallback — so a code
        // that happens to collide with another item's barcode never steals
        // the lookup out from under it.
        const match = items.find((item) => item.barcode === code) ?? items.find((item) => item.shortCode === code);
        // `addItem` itself silently no-ops for an out-of-stock item (same as
        // the mouse "Add to Cart" path) — only move the active-line
        // selection onto it when it actually landed in the cart.
        if (match && match.stockQty > 0) {
          addItem(match);
          setActiveLine(match.id);
        }
        return;
      }

      // Enter with an empty buffer and a soldByAmount line selected: open
      // its amount popup. A non-soldByAmount active line (or nothing
      // selected) leaves Enter a no-op here — there's no amount to type for
      // a per-piece line.
      if (event.key === "Enter" && buffer === "" && activeLineItemId !== null) {
        const activeItem = items.find((item) => item.id === activeLineItemId);
        if (activeItem?.soldByAmount) {
          event.preventDefault();
          requestAmountEntry(activeItem);
        }
        return;
      }
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    enabled,
    items,
    tablesEnabled,
    tableId,
    buffer,
    tablePopupOpen,
    showHelp,
    activeLineItemId,
    onPlaceOrder,
    onPrintToken,
  ]);

  return {
    buffer,
    tablePopupOpen,
    closeTablePopup: () => setTablePopupOpen(false),
    showHelp,
    closeHelp: () => setShowHelp(false),
  };
}
