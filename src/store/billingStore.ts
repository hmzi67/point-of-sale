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

  addItem: (item: Item) => void;
  setQty: (itemId: number, qty: number) => void;
  removeItem: (itemId: number) => void;
  setDiscountMode: (mode: DiscountMode) => void;
  setDiscountValue: (value: number) => void;
  setPaymentMethod: (method: PaymentMethod) => void;
  setTableId: (tableId: number | null) => void;
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
};

export const useBillingStore = create<BillingState>((set, get) => ({
  ...initialCartState,
  paymentMethod: "cash",

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
        qty: 1,
      };
      return {
        cart: { ...state.cart, [item.id]: entry },
        cartOrder: [...state.cartOrder, item.id],
      };
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
        qty: Math.min(line.qty, Math.max(item.stockQty, 1)),
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

  clearCart: () => set({ ...initialCartState, paymentMethod: get().paymentMethod }),
}));
