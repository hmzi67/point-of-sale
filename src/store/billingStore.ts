import { create } from "zustand";
import type { Item, ParkedCartLine, PaymentMethod } from "../types";

export interface CartEntry {
  itemId: number;
  name: string;
  barcode: string | null;
  /** Price at the moment the item was added — a live re-price still happens
   * server-side at checkout, this is only for the on-screen running total. */
  priceMinor: number;
  stockQty: number;
  qty: number;
  /** Filename under the product-image store, for the cart row's thumbnail —
   * same field `Item.imagePath` carries, just copied onto the cart line. */
  imagePath: string | null;
  /** A cashier's free-text note on this line (e.g. "no onions"). Empty
   * string, not `null`, when unset — matches every text `<input>`'s value. */
  notes: string;
  /** Copied from `Item.soldByAmount` at add time — lets the cart row and
   * `setQty`/`updateLine`'s clamping tell a normal per-piece line (whole
   * numbers only) from a loose/weighed one (fractional qty allowed). */
  soldByAmount: boolean;
  /** Copied from `Item.unit` — display only, e.g. "kg" on the cart row. */
  unit: string | null;
}

/** The smallest a line's qty may ever clamp down to — 1 whole unit for a
 * normal item, a token 0.01 for a `soldByAmount` line (there's no
 * meaningful "1" for a fractional-quantity item, and a cashier must still
 * be able to fine-tune it down after weighing on a scale). */
function minQty(entry: Pick<CartEntry, "soldByAmount">): number {
  return entry.soldByAmount ? 0.01 : 1;
}

/** The keyboard fast-billing arrow-key qty step — always a whole unit, same
 * as the mouse +/- stepper, for every line including a `soldByAmount` one.
 * Left/Right is a quantity control, not the amount one — typing a rupee
 * amount is what the Enter popup is for (see `ItemAmountEntryModal`); the
 * arrow keys just nudge the piece count up/down from there. */
function qtyStep(): number {
  return 1;
}

export type DiscountMode = "flat" | "percent";

interface BillingState {
  /** Keyed by itemId, not an array — a `CartRow` subscribes to its own entry
   * (`state.cart[itemId]`) so editing one line's quantity never re-renders
   * every other row. `cartOrder` is the stable id list the cart panel maps
   * over for row order, kept separate so it only changes on add/remove. */
  cart: Record<number, CartEntry>;
  cartOrder: number[];

  /** The keyboard fast-billing "active line" — highlighted in the cart panel,
   * and the target of the arrow-key qty adjust and Delete/Backspace remove
   * shortcuts. `null` when nothing's selected (empty cart, or the active
   * line was just removed). Not touched by mouse interactions — a cashier
   * clicking a stepper doesn't fight the keyboard flow for this. */
  activeLineItemId: number | null;

  discountMode: DiscountMode;
  /** Raw input value: a currency amount when `discountMode` is "flat", a
   * whole percentage 0–100 when "percent". */
  discountValue: number;

  paymentMethod: PaymentMethod;
  /** Selected dine-in table, or `null` for a counter sale. Only meaningful
   * when the `tables` module is enabled. */
  tableId: number | null;

  /** Cashier-typed label for the current order (e.g. a customer's name).
   * Empty string falls back to a generic display like "Counter Sale" in the
   * UI — there's no customer-management table behind this, it's just a
   * label attached to this one draft order. */
  customerName: string;
  /** A local, session-only counter — not a real order/sale id (we don't have
   * one until `billing_create_sale` actually returns a `Sale`). Increments
   * every time the current draft is cleared (completed, parked, or reset),
   * purely so the checkout header has *something* to show before a sale id
   * exists. */
  draftOrderNumber: number;

  /** Quick-add: +1 to an existing line (keeping its notes), or a fresh line
   * at qty 1. Used by the barcode-scan/search path and the item card's
   * "Add to Cart" button — no mouse, no modal. */
  addItem: (item: Item) => void;
  /** "By Amount" entry for a `soldByAmount` item: computes qty from a
   * rupee amount (amountMinor ÷ item.priceMinor, rounded to 2dp — see
   * `ItemAmountEntryModal`) and SETS the line to exactly that qty —
   * replacing whatever qty was there, not adding to it. The normal trigger
   * (`useFastBillingHotkeys`'s Enter-on-active-line) only ever fires for a
   * line already in the cart (added at qty 0 by `addItem`), but this also
   * creates the line if it somehow isn't there yet, at that same computed
   * qty. Does nothing for an item that isn't `soldByAmount` — the caller
   * only ever opens the popup for one, but this is the same defensive floor
   * `addItem` has via `item.stockQty <= 0`. */
  addItemByAmount: (item: Item, amountMinor: number) => void;
  /** The cart row's notes-pencil "edit" flow: sets (not adds) qty and notes
   * to exactly the given values, since this is editing an existing line
   * rather than adding more of it. */
  updateLine: (itemId: number, qty: number, notes: string) => void;
  setQty: (itemId: number, qty: number) => void;
  removeItem: (itemId: number) => void;

  /** Keyboard fast-billing: selects one cart line as "active" (or clears the
   * selection with `null`). */
  setActiveLine: (itemId: number | null) => void;
  /** Moves the active selection up (`-1`) or down (`1`) through `cartOrder`.
   * No active line yet picks the first (down) or last (up) line; already at
   * an end just stays put rather than wrapping, matching how a spreadsheet's
   * arrow keys behave at a sheet's edge. A no-op on an empty cart. */
  moveActiveLine: (direction: -1 | 1) => void;
  /** Adjusts the active line's qty by one step (see `qtyStep`) in the given
   * direction, reusing the same stock-clamp/remove-at-floor rules the mouse
   * +/- stepper already has. A no-op with nothing active. */
  adjustActiveQty: (direction: -1 | 1) => void;
  /** Removes the active line entirely (same effect as the row's trash icon)
   * and clears the selection. A no-op with nothing active. */
  removeActiveLine: () => void;
  setDiscountMode: (mode: DiscountMode) => void;
  setDiscountValue: (value: number) => void;
  setPaymentMethod: (method: PaymentMethod) => void;
  setTableId: (tableId: number | null) => void;
  setCustomerName: (name: string) => void;
  /** Replaces the cart with a resumed table order's lines. */
  loadParkedCart: (lines: ParkedCartLine[], discountMinor: number, resolveItem: (id: number) => Item | undefined) => void;
  clearCart: () => void;

  /** Which `soldByAmount` item's amount-entry popup is currently open, if
   * any. Transient UI state, not cart data — lives here (rather than as
   * local state in whichever component triggered it) because the trigger
   * (`useFastBillingHotkeys`'s Enter-on-active-line — select a soldByAmount
   * line already in the cart, press Enter) isn't an ancestor of the modal;
   * `BillingPage` mounts the one `ItemAmountEntryModal` that reads this.
   * Same reasoning `tableId`/`customerName` above already established for
   * billing-UI state that isn't itself a cart line. */
  amountEntryItem: Item | null;
  requestAmountEntry: (item: Item) => void;
  cancelAmountEntry: () => void;
}

const initialCartState = {
  cart: {} as Record<number, CartEntry>,
  cartOrder: [] as number[],
  activeLineItemId: null as number | null,
  discountMode: "flat" as DiscountMode,
  discountValue: 0,
  tableId: null as number | null,
  customerName: "",
};

export const useBillingStore = create<BillingState>((set) => ({
  ...initialCartState,
  paymentMethod: "cash",
  draftOrderNumber: 1,
  amountEntryItem: null,

  addItem: (item) => {
    if (item.stockQty <= 0) return;

    set((state) => {
      const existing = state.cart[item.id];
      if (existing) {
        // A soldByAmount line's qty is only ever set via the amount popup
        // (Enter on the active cart line) — a repeat card click while it's
        // already in the cart just leaves it alone rather than silently
        // stacking a meaningless "+1 unit" onto a weighed/loose line.
        if (item.soldByAmount) return state;
        const qty = Math.min(existing.qty + 1, item.stockQty);
        return { cart: { ...state.cart, [item.id]: { ...existing, qty } } };
      }

      const entry: CartEntry = {
        itemId: item.id,
        name: item.name,
        barcode: item.barcode,
        priceMinor: item.priceMinor,
        stockQty: item.stockQty,
        imagePath: item.imagePath,
        // A soldByAmount item starts at 0, not 1 — the card's single "Add
        // to Cart" button just puts it on the ticket; the cashier then
        // selects the line and presses Enter to type the actual amount
        // (see `useFastBillingHotkeys`/`ItemAmountEntryModal`). A plain
        // per-piece item still starts at 1, same as always.
        qty: item.soldByAmount ? 0 : 1,
        notes: "",
        soldByAmount: item.soldByAmount,
        unit: item.unit,
      };
      return {
        cart: { ...state.cart, [item.id]: entry },
        cartOrder: [...state.cartOrder, item.id],
      };
    });
  },

  addItemByAmount: (item, amountMinor) => {
    if (!item.soldByAmount || item.priceMinor <= 0 || amountMinor <= 0 || item.stockQty <= 0) return;

    // Deliberately NOT rounded. The server charges `round(price × qty)`
    // (see `db::sales::create_sale`), so any rounding of qty here lands
    // straight on the customer's bill: at 300.00/unit, a typed 100 became
    // qty 0.33 and billed 99.00, and a typed 50 became qty 0.17 and billed
    // *51.00* — the error ran in both directions, overcharging as readily
    // as undercharging.
    //
    // Carrying the full-precision quotient makes `price × qty` reduce back
    // to the typed amount exactly: f64's relative error here is ~1e-16, so
    // the product lands within a billionth of a minor unit of the amount and
    // `.round()` recovers it precisely. Display is unaffected — `formatQty`
    // and `printer::layout::format_qty` still show 2dp — and `sale_items.qty`
    // / `items.stock_qty` are both REAL, so the exact figure survives
    // storage and the stock decrement.
    const rawQty = amountMinor / item.priceMinor;
    const qty = Math.min(rawQty, item.stockQty);

    set((state) => {
      const existing = state.cart[item.id];
      if (existing) {
        // SET, not add — typing 50 always means "this line is worth PKR 50
        // now", regardless of whatever qty (0, or a previously-set amount)
        // was there before.
        return { cart: { ...state.cart, [item.id]: { ...existing, qty } } };
      }

      const entry: CartEntry = {
        itemId: item.id,
        name: item.name,
        barcode: item.barcode,
        priceMinor: item.priceMinor,
        stockQty: item.stockQty,
        imagePath: item.imagePath,
        qty,
        notes: "",
        soldByAmount: item.soldByAmount,
        unit: item.unit,
      };
      return {
        cart: { ...state.cart, [item.id]: entry },
        cartOrder: [...state.cartOrder, item.id],
      };
    });
  },

  updateLine: (itemId, qty, notes) => {
    set((state) => {
      const existing = state.cart[itemId];
      if (!existing) return state;
      const clamped = Math.max(minQty(existing), Math.min(qty, existing.stockQty));
      return { cart: { ...state.cart, [itemId]: { ...existing, qty: clamped, notes } } };
    });
  },

  setQty: (itemId, qty) => {
    set((state) => {
      const existing = state.cart[itemId];
      if (!existing) return state;
      const clamped = Math.max(minQty(existing), Math.min(qty, existing.stockQty));
      return { cart: { ...state.cart, [itemId]: { ...existing, qty: clamped } } };
    });
  },

  removeItem: (itemId) => {
    set((state) => {
      const { [itemId]: _removed, ...rest } = state.cart;
      return {
        cart: rest,
        cartOrder: state.cartOrder.filter((id) => id !== itemId),
        activeLineItemId: state.activeLineItemId === itemId ? null : state.activeLineItemId,
      };
    });
  },

  setActiveLine: (itemId) => set({ activeLineItemId: itemId }),

  moveActiveLine: (direction) => {
    set((state) => {
      if (state.cartOrder.length === 0) return state;
      const currentIndex = state.activeLineItemId === null ? -1 : state.cartOrder.indexOf(state.activeLineItemId);

      let nextIndex: number;
      if (currentIndex === -1) {
        nextIndex = direction === 1 ? 0 : state.cartOrder.length - 1;
      } else {
        nextIndex = Math.max(0, Math.min(state.cartOrder.length - 1, currentIndex + direction));
      }

      return { activeLineItemId: state.cartOrder[nextIndex] };
    });
  },

  adjustActiveQty: (direction) => {
    set((state) => {
      if (state.activeLineItemId === null) return state;
      const entry = state.cart[state.activeLineItemId];
      if (!entry) return state;

      const step = qtyStep();
      const nextQty = Math.round((entry.qty + direction * step) * 100) / 100;

      // Same floor behavior as the mouse stepper (CartRow/ItemCard): dropping
      // at or below the minimum removes the line outright rather than
      // clamping to a value the cashier didn't ask for.
      if (nextQty < minQty(entry)) {
        const { [state.activeLineItemId]: _removed, ...rest } = state.cart;
        return {
          cart: rest,
          cartOrder: state.cartOrder.filter((id) => id !== state.activeLineItemId),
          activeLineItemId: null,
        };
      }

      const clamped = Math.min(nextQty, entry.stockQty);
      return { cart: { ...state.cart, [state.activeLineItemId]: { ...entry, qty: clamped } } };
    });
  },

  removeActiveLine: () => {
    set((state) => {
      if (state.activeLineItemId === null) return state;
      const { [state.activeLineItemId]: _removed, ...rest } = state.cart;
      return {
        cart: rest,
        cartOrder: state.cartOrder.filter((id) => id !== state.activeLineItemId),
        activeLineItemId: null,
      };
    });
  },

  setDiscountMode: (discountMode) => set({ discountMode, discountValue: 0 }),
  setDiscountValue: (discountValue) => set({ discountValue: Math.max(0, discountValue) }),
  setPaymentMethod: (paymentMethod) => set({ paymentMethod }),
  setTableId: (tableId) => set({ tableId }),
  setCustomerName: (customerName) => set({ customerName }),

  loadParkedCart: (lines, discountMinor, resolveItem) => {
    const cart: Record<number, CartEntry> = {};
    const cartOrder: number[] = [];

    for (const line of lines) {
      const item = resolveItem(line.itemId);
      if (!item) continue; // item was deleted since the cart was parked
      cart[line.itemId] = {
        itemId: line.itemId,
        name: item.name,
        barcode: item.barcode,
        priceMinor: item.priceMinor,
        stockQty: item.stockQty,
        imagePath: item.imagePath,
        qty: Math.min(line.qty, Math.max(item.stockQty, item.soldByAmount ? 0.01 : 1)),
        // A parked table order doesn't carry per-line notes today (see
        // `ParkedCartLine` on the Rust side) — resuming one just starts
        // each line with no note, same as before this feature existed.
        notes: "",
        soldByAmount: item.soldByAmount,
        unit: item.unit,
      };
      cartOrder.push(line.itemId);
    }

    set({
      cart,
      cartOrder,
      activeLineItemId: null,
      discountMode: "flat",
      discountValue: discountMinor / 100,
    });
  },

  clearCart: () =>
    set((state) => ({
      ...initialCartState,
      paymentMethod: state.paymentMethod,
      draftOrderNumber: state.draftOrderNumber + 1,
    })),

  requestAmountEntry: (item) => set({ amountEntryItem: item }),
  cancelAmountEntry: () => set({ amountEntryItem: null }),
}));
