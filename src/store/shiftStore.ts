import { create } from "zustand";
import { closeShift, getOpenShift, openShift } from "../services/shiftsService";
import type { Shift, ShiftSummary } from "../types";

/**
 * The signed-in cashier's currently-open shift (if any), for the Billing
 * screen's Open/Close Shift affordance. Only meaningful when the `shifts`
 * module is enabled — `BillingHeader` doesn't render anything from this
 * store when it isn't.
 */
interface ShiftStoreState {
  openShift: Shift | null;
  isLoaded: boolean;
  isLoading: boolean;
  error: string | null;
  load: () => Promise<void>;
  open: (openingBalanceMinor: number) => Promise<void>;
  /** Resolves with the final reconciliation summary so the caller can show
   * it (and offer to print) before dismissing. */
  close: (declaredCashAmountMinor: number) => Promise<ShiftSummary>;
  clearError: () => void;
}

export const useShiftStore = create<ShiftStoreState>((set, get) => ({
  openShift: null,
  isLoaded: false,
  isLoading: false,
  error: null,

  load: async () => {
    set({ isLoading: true, error: null });
    try {
      const shift = await getOpenShift();
      set({ openShift: shift, isLoaded: true, isLoading: false });
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false, isLoaded: true });
    }
  },

  open: async (openingBalanceMinor) => {
    set({ error: null });
    try {
      const shift = await openShift(openingBalanceMinor);
      set({ openShift: shift });
    } catch (error) {
      set({ error: (error as Error).message });
      throw error;
    }
  },

  close: async (declaredCashAmountMinor) => {
    const shift = get().openShift;
    if (!shift) throw new Error("No open shift to close");
    set({ error: null });
    try {
      const summary = await closeShift(shift.id, declaredCashAmountMinor);
      set({ openShift: null });
      return summary;
    } catch (error) {
      set({ error: (error as Error).message });
      throw error;
    }
  },

  clearError: () => set({ error: null }),
}));
