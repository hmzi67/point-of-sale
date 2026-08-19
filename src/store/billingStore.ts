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
}

export type DiscountMode = "flat" | "percent";

interface BillingState {
  /** Keyed by itemId, not an array — a `CartRow` subscribes to its own entry
   * (`state.cart[itemId]`) so editing one line's quantity never re-renders
   * every other row. `cartOrder` is the stable id list the cart panel maps
   * over for row order, kept separate so it only changes on add/remove. */
  cart: Record<number, CartEntry>;
  cartOrder: number[];

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
   * at qty 1. Used by the barcode-scan/search path — no mouse, no modal. */
  addItem: (item: Item) => void;
  /** The item detail modal's "Add to Cart": adds `qty` on top of whatever's
   * already in the cart for this item, and overwrites the line's notes with
   * whatever was typed in the modal this time. */
  addItemWithDetails: (item: Item, qty: number, notes: string) => void;
  /** The cart row's notes-pencil "edit" flow: sets (not adds) qty and notes
   * to exactly the given values, since this is editing an existing line
   * rather than adding more of it. */
  updateLine: (itemId: number, qty: number, notes: string) => void;
  setQty: (itemId: number, qty: number) => void;
  removeItem: (itemId: number) => void;
  setDiscountMode: (mode: DiscountMode) => void;
  setDiscountValue: (value: number) => void;
  setPaymentMethod: (method: PaymentMethod) => void;
  setTableId: (tableId: number | null) => void;
  setCustomerName: (name: string) => void;
  /** Replaces the cart with a resumed table order's lines. */
  loadParkedCart: (lines: ParkedCartLine[], discountMinor: number, resolveItem: (id: number) => Item | undefined) => void;
  clearCart: () => void;
}

const initialCartState = {
  cart: {} as Record<number, CartEntry>,
  cartOrder: [] as number[],
  discountMode: "flat" as DiscountMode,
  discountValue: 0,
  tableId: null as number | null,
  customerName: "",
};

export const useBillingStore = create<BillingState>((set) => ({
  ...initialCartState,
  paymentMethod: "cash",
  draftOrderNumber: 1,

  addItem: (item) => {
    if (item.stockQty <= 0) return;

    set((state) => {
      const existing = state.cart[item.id];
      if (existing) {
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
        qty: 1,
        notes: "",
      };
      return {
        cart: { ...state.cart, [item.id]: entry },
        cartOrder: [...state.cartOrder, item.id],
      };
    });
  },

  addItemWithDetails: (item, qty, notes) => {
    if (item.stockQty <= 0 || qty <= 0) return;

    set((state) => {
      const existing = state.cart[item.id];
      if (existing) {
        const nextQty = Math.min(existing.qty + qty, item.stockQty);
        return { cart: { ...state.cart, [item.id]: { ...existing, qty: nextQty, notes } } };
      }

      const entry: CartEntry = {
        itemId: item.id,
        name: item.name,
        barcode: item.barcode,
        priceMinor: item.priceMinor,
        stockQty: item.stockQty,
        imagePath: item.imagePath,
        qty: Math.min(qty, item.stockQty),
        notes,
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
      const clamped = Math.max(1, Math.min(qty, existing.stockQty));
      return { cart: { ...state.cart, [itemId]: { ...existing, qty: clamped, notes } } };
    });
  },

  setQty: (itemId, qty) => {
    set((state) => {
      const existing = state.cart[itemId];
      if (!existing) return state;
      const clamped = Math.max(1, Math.min(qty, existing.stockQty));
      return { cart: { ...state.cart, [itemId]: { ...existing, qty: clamped } } };
    });
  },

  removeItem: (itemId) => {
    set((state) => {
      const { [itemId]: _removed, ...rest } = state.cart;
      return { cart: rest, cartOrder: state.cartOrder.filter((id) => id !== itemId) };
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
        qty: Math.min(line.qty, Math.max(item.stockQty, 1)),
        // A parked table order doesn't carry per-line notes today (see
        // `ParkedCartLine` on the Rust side) — resuming one just starts
        // each line with no note, same as before this feature existed.
        notes: "",
      };
      cartOrder.push(line.itemId);
    }

    set({
      cart,
      cartOrder,
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
}));
