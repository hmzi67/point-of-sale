import { create } from "zustand";
import { login as loginCommand, logout as logoutCommand } from "../services/authService";
import { reportError } from "../services/logService";
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

  logout: () => {
    // Clear the local session immediately for a snappy sign-out; the Rust
    // side (the one that actually gates sensitive commands) is cleared right
    // behind it. Not awaited — a failed IPC call here shouldn't block the
    // cashier from walking away from the till — but a failure is retried
    // once and then logged, since it would leave the Rust session signed in.
    set({ user: null, error: null });
    logoutCommand()
      .catch(() => logoutCommand())
      .catch((error: Error) => reportError(`[auth] sign-out did not reach the backend: ${error.message}`));
  },
  clearError: () => set({ error: null }),
}));
