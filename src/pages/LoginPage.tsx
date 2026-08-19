import { useEffect, useState } from "react";
import { Navigate } from "react-router-dom";
import { Delete, Store } from "lucide-react";
import { getUsers } from "../services/authService";
import { useAuthStore } from "../store";
import { PIN_MAX_LENGTH, PIN_MIN_LENGTH, type User } from "../types";

const KEYPAD = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/**
 * PIN login. Keypad-first because the till may be a touchscreen, but the
 * physical number row works too — a cashier should never need the mouse.
 */
export function LoginPage() {
  const [users, setUsers] = useState<User[]>([]);
  const [selectedUserId, setSelectedUserId] = useState<number | null>(null);
  const [pin, setPin] = useState("");
  const [loadError, setLoadError] = useState<string | null>(null);

  const currentUser = useAuthStore((state) => state.user);
  const login = useAuthStore((state) => state.login);
  const isAuthenticating = useAuthStore((state) => state.isAuthenticating);
  const error = useAuthStore((state) => state.error);
  const clearError = useAuthStore((state) => state.clearError);

  useEffect(() => {
    getUsers()
      .then((loaded) => {
        setUsers(loaded);
        setSelectedUserId((current) => current ?? loaded[0]?.id ?? null);
      })
      .catch((e: Error) => setLoadError(e.message));
  }, []);

  const submit = async () => {
    if (selectedUserId === null || pin.length < PIN_MIN_LENGTH) return;
    await login(selectedUserId, pin);
    setPin("");
  };

  const press = (digit: string) => {
    clearError();
    setPin((current) => (current.length >= PIN_MAX_LENGTH ? current : current + digit));
  };

  // Physical keyboard: digits type, Backspace deletes, Enter submits.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (/^[0-9]$/.test(event.key)) press(event.key);
      else if (event.key === "Backspace") setPin((c) => c.slice(0, -1));
      else if (event.key === "Enter") void submit();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  });

  if (currentUser) return <Navigate to="/" replace />;

  return (
    <div className="flex h-full items-center justify-center bg-slate-900 p-6">
      <div className="w-full max-w-sm rounded-xl bg-white p-6 shadow-xl">
        <div className="mb-6 flex items-center gap-2">
          <Store className="h-6 w-6 text-brand-600" />
          <h1 className="text-lg font-semibold text-slate-900">Sign in</h1>
        </div>

        {loadError && (
          <p className="mb-4 rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{loadError}</p>
        )}

        <label className="block text-xs font-medium text-slate-500" htmlFor="user">
          User
        </label>
        <select
          id="user"
          value={selectedUserId ?? ""}
          onChange={(event) => {
            clearError();
            setPin("");
            setSelectedUserId(Number(event.target.value));
          }}
          className="mt-1 w-full rounded-md border border-slate-300 px-3 py-2 text-sm"
        >
          {users.map((user) => (
            <option key={user.id} value={user.id}>
              {user.name} · {user.role}
            </option>
          ))}
        </select>

        <div className="mt-5 flex justify-center gap-3">
          {Array.from({ length: PIN_MAX_LENGTH }).map((_, index) => (
            <span
              key={index}
              className={[
                "h-3 w-3 rounded-full",
                index < pin.length ? "bg-brand-600" : "bg-slate-200",
              ].join(" ")}
            />
          ))}
        </div>

        {error && <p className="mt-4 text-center text-sm text-red-600">{error}</p>}

        <div className="mt-5 grid grid-cols-3 gap-2">
          {KEYPAD.map((digit) => (
            <button
              key={digit}
              type="button"
              onClick={() => press(digit)}
              className="rounded-md bg-slate-100 py-3 text-lg font-medium text-slate-900 hover:bg-slate-200"
            >
              {digit}
            </button>
          ))}
          <button
            type="button"
            onClick={() => setPin((c) => c.slice(0, -1))}
            className="flex items-center justify-center rounded-md bg-slate-100 py-3 text-slate-600 hover:bg-slate-200"
            aria-label="Delete last digit"
          >
            <Delete className="h-5 w-5" />
          </button>
          <button
            type="button"
            onClick={() => press("0")}
            className="rounded-md bg-slate-100 py-3 text-lg font-medium text-slate-900 hover:bg-slate-200"
          >
            0
          </button>
          <button
            type="button"
            onClick={() => void submit()}
            disabled={isAuthenticating || pin.length < PIN_MIN_LENGTH || selectedUserId === null}
            className="rounded-md bg-brand-600 py-3 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-40"
          >
            {isAuthenticating ? "…" : "Enter"}
          </button>
        </div>

        <p className="mt-5 text-center text-xs text-slate-400">
          First run? Sign in as Owner with PIN 1234, then change it.
        </p>
      </div>
    </div>
  );
}
