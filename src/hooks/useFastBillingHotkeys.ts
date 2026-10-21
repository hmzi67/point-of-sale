import { useEffect, useRef, useState } from "react";
import { useBillingStore } from "../store";
import type { Item } from "../types";
import { findItemByCode } from "../utils/itemLookup";

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
export type HotkeyAction =
  | { type: "toggleHelp" }
  | { type: "closeHelp" }
  | { type: "placeOrder" }
  | { type: "printToken" }
  | { type: "clearBuffer" }
  | { type: "moveLine"; delta: 1 | -1 }
  | { type: "adjustQty"; delta: 1 | -1 }
  | { type: "removeLine" }
  | { type: "openTablePopup" }
  | { type: "appendDigit"; digit: string }
  | { type: "lookupBuffer" }
  | { type: "amountEntry" };

export interface HotkeyContext {
  textEntryFocused: boolean;
  showHelp: boolean;
  buffer: string;
  tablesEnabled: boolean;
  tablePopupOpen: boolean;
  hasActiveLine: boolean;
}

/**
 * What one keydown means, given the screen's state — pure, so the rules in
 * the doc comment below are unit-tested. `null` means "not ours": leave the
 * event alone. `mod` is Ctrl, or Cmd on macOS.
 */
export function resolveHotkey(key: string, mod: boolean, ctx: HotkeyContext): HotkeyAction | null {
  // "?" works even mid-buffer — a short code never contains it.
  if (key === "?" && !ctx.textEntryFocused) return { type: "toggleHelp" };
  // The help overlay swallows everything but Escape, so nothing edits the cart behind it.
  if (ctx.showHelp) return key === "Escape" ? { type: "closeHelp" } : null;
  // A real text field (search, discount, notes, the table popup's input) is never hijacked.
  if (ctx.textEntryFocused) return null;

  // Mod combos are checked before the buffer, so they are never typed into it.
  if (key === "Enter" && mod) return { type: "placeOrder" };
  // Ctrl+K only has a target when the Tables module mounts `OrderTypeAndTable`.
  if ((key === "k" || key === "K") && mod) return ctx.tablesEnabled ? { type: "printToken" } : null;

  switch (key) {
    case "Escape":
      return { type: "clearBuffer" };
    case "ArrowUp":
      return { type: "moveLine", delta: -1 };
    case "ArrowDown":
      return { type: "moveLine", delta: 1 };
    case "ArrowRight":
      return { type: "adjustQty", delta: 1 };
    case "ArrowLeft":
      return { type: "adjustQty", delta: -1 };
    case "Delete":
    case "Backspace":
      return { type: "removeLine" };
  }

  if ((key === "t" || key === "T") && ctx.buffer === "" && ctx.tablesEnabled && !ctx.tablePopupOpen) {
    return { type: "openTablePopup" };
  }
  if (/^[0-9]$/.test(key)) return { type: "appendDigit", digit: key };
  if (key === "Enter" && ctx.buffer !== "") return { type: "lookupBuffer" };
  if (key === "Enter" && ctx.hasActiveLine) return { type: "amountEntry" };
  return null;
}

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
      const action = resolveHotkey(event.key, event.metaKey || event.ctrlKey, {
        textEntryFocused: isTextEntryFocused(),
        showHelp,
        buffer,
        tablesEnabled,
        tablePopupOpen,
        hasActiveLine: activeLineItemId !== null,
      });
      if (action === null) return;
      // Escape only clears/closes; the amount popup decides below whether Enter is consumed.
      if (action.type !== "clearBuffer" && action.type !== "closeHelp" && action.type !== "amountEntry") {
        event.preventDefault();
      }

      switch (action.type) {
        case "toggleHelp":
          setShowHelp((v) => !v);
          return;
        case "closeHelp":
          setShowHelp(false);
          return;
        case "placeOrder":
          onPlaceOrder();
          return;
        case "printToken":
          onPrintToken();
          return;
        case "clearBuffer":
          setBuffer("");
          return;
        case "moveLine":
          moveActiveLine(action.delta);
          return;
        case "adjustQty":
          adjustActiveQty(action.delta);
          return;
        case "removeLine":
          removeActiveLine();
          return;
        case "openTablePopup":
          setTablePopupOpen(true);
          return;
        case "appendDigit":
          setBuffer((current) => (current + action.digit).slice(0, 6));
          clearBufferSoon();
          return;
        case "lookupBuffer": {
          const match = findItemByCode(items, buffer);
          setBuffer("");
          // `addItem` itself silently no-ops for an out-of-stock item (same as
          // the mouse "Add to Cart" path) — only move the active-line
          // selection onto it when it actually landed in the cart.
          if (match && match.stockQty > 0) {
            addItem(match);
            setActiveLine(match.id);
          }
          return;
        }
        case "amountEntry": {
          // Only a soldByAmount line has an amount to type; Enter is a no-op otherwise.
          const activeItem = items.find((item) => item.id === activeLineItemId);
          if (activeItem?.soldByAmount) {
            event.preventDefault();
            requestAmountEntry(activeItem);
          }
          return;
        }
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
