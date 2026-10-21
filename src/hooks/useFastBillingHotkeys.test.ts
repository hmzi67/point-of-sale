import { describe, expect, it } from "vitest";
import { resolveHotkey, type HotkeyContext } from "./useFastBillingHotkeys";

const idle: HotkeyContext = {
  textEntryFocused: false,
  showHelp: false,
  buffer: "",
  tablesEnabled: true,
  tablePopupOpen: false,
  hasActiveLine: false,
};

describe("resolveHotkey", () => {
  it("never hijacks a focused text field, except for nothing at all", () => {
    for (const key of ["1", "Enter", "t", "Backspace", "?", "ArrowUp"]) {
      expect(resolveHotkey(key, false, { ...idle, textEntryFocused: true }), key).toBeNull();
    }
    expect(resolveHotkey("Enter", true, { ...idle, textEntryFocused: true })).toBeNull();
  });

  it("lets only Escape through while the help overlay is open", () => {
    const ctx = { ...idle, showHelp: true, hasActiveLine: true };
    expect(resolveHotkey("Escape", false, ctx)).toEqual({ type: "closeHelp" });
    expect(resolveHotkey("Delete", false, ctx)).toBeNull();
    expect(resolveHotkey("?", false, ctx)).toEqual({ type: "toggleHelp" });
  });

  it("places the order on Ctrl/Cmd+Enter even with a code half-typed", () => {
    expect(resolveHotkey("Enter", true, { ...idle, buffer: "12" })).toEqual({ type: "placeOrder" });
  });

  it("prints a token on Ctrl/Cmd+K only when tables are enabled", () => {
    expect(resolveHotkey("k", true, idle)).toEqual({ type: "printToken" });
    expect(resolveHotkey("K", true, { ...idle, tablesEnabled: false })).toBeNull();
  });

  it("builds a buffer from digits and looks it up on Enter", () => {
    expect(resolveHotkey("7", false, idle)).toEqual({ type: "appendDigit", digit: "7" });
    expect(resolveHotkey("Enter", false, { ...idle, buffer: "7" })).toEqual({ type: "lookupBuffer" });
  });

  it("opens the table popup on T only with an empty buffer and tables on", () => {
    expect(resolveHotkey("t", false, idle)).toEqual({ type: "openTablePopup" });
    expect(resolveHotkey("t", false, { ...idle, buffer: "1" })).toBeNull();
    expect(resolveHotkey("T", false, { ...idle, tablesEnabled: false })).toBeNull();
    expect(resolveHotkey("t", false, { ...idle, tablePopupOpen: true })).toBeNull();
  });

  it("uses plain Enter on an empty buffer for the active line's amount, else ignores it", () => {
    expect(resolveHotkey("Enter", false, { ...idle, hasActiveLine: true })).toEqual({ type: "amountEntry" });
    expect(resolveHotkey("Enter", false, idle)).toBeNull();
  });

  it("maps arrows and delete to cart-line edits", () => {
    expect(resolveHotkey("ArrowUp", false, idle)).toEqual({ type: "moveLine", delta: -1 });
    expect(resolveHotkey("ArrowLeft", false, idle)).toEqual({ type: "adjustQty", delta: -1 });
    expect(resolveHotkey("Backspace", false, idle)).toEqual({ type: "removeLine" });
  });
});
