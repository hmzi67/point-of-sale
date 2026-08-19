import { create } from "zustand";
import { login as loginCommand } from "../services/authService";
import type { User } from "../types";

/**
 * Session state, in memory only. This is a single-machine offline app: there
 * are no tokens to persist, and closing the app should end the shift.
 */
interface AuthState {
  user: User | null;
  isAuthenticating: boolean;
  error: string | null;
  login: (userId: number, pin: string) => Promise<void>;
  logout: () => void;
  clearError: () => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  user: null,
  isAuthenticating: false,
  error: null,

  login: async (userId, pin) => {
    set({ isAuthenticating: true, error: null });
    try {
      const user = await loginCommand(userId, pin);
      set({ user, isAuthenticating: false });
    } catch (error) {
      set({ error: (error as Error).message, isAuthenticating: false, user: null });
    }
  },

  logout: () => set({ user: null, error: null }),
  clearError: () => set({ error: null }),
}));
