import { lazy, Suspense, useEffect, useState } from "react";
import { Navigate } from "react-router-dom";
import { Delete, Store } from "lucide-react";
import { DropdownSelect } from "../components/ui/DropdownSelect";
import { getUsers } from "../services/authService";
import { useSecretTapTrigger } from "../hooks/useSecretTapTrigger";
import { useAuthStore } from "../store";
import { PIN_MAX_LENGTH, PIN_MIN_LENGTH, type Role, type User } from "../types";

// Lazy so this never loads for the overwhelming majority of app sessions
// that never trigger it — see `useSecretTapTrigger`'s doc comment.
const ProductOwnerModal = lazy(() =>
  import("../components/productOwner/ProductOwnerModal").then((m) => ({ default: m.ProductOwnerModal })),
);

const KEYPAD = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

/** Role badge tint — same "tone conveys meaning at a glance" idea the rest
 * of the app uses for status dots/pills, applied to the user dropdown's
 * `meta` slot so Owner/Admin/Cashier read apart instantly in the list. */
const ROLE_TONE: Record<Role, string> = {
  owner: "bg-amber-50 text-amber-700",
  admin: "bg-brand-50 text-brand-700",
  cashier: "bg-slate-100 text-slate-600",
};

/**
 * PIN login. Keypad-first because the till may be a touchscreen, but the
 * physical number row works too — a cashier should never need the mouse.
 *
 * Static `/brand-logo.png` only (not `useAppConfig()`'s client-configured
 * branding) — installation config deliberately doesn't load until after
 * sign-in (see `App.tsx`'s `useBootstrap`), precisely so this screen stays
 * usable even if a config read fails. The bundled brand mark is safe here
 * because it's a static asset, not a DB round-trip.
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

  const [showVendorAccess, setShowVendorAccess] = useState(false);
  const onSecretTap = useSecretTapTrigger(() => setShowVendorAccess(true));

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
    <div className="relative flex h-full items-center justify-center overflow-hidden bg-gradient-to-br from-brand-950 via-brand-900 to-slate-900 p-6">
      {/* Purely decorative soft glow — same brand hue as the rest of the
          app, just here to keep an otherwise flat dark screen from feeling
          bare. Doesn't affect layout or contrast of anything above it. */}
      <div className="pointer-events-none absolute -left-32 -top-32 h-80 w-80 rounded-full bg-brand-600/30 blur-3xl" />
      <div className="pointer-events-none absolute -bottom-32 -right-24 h-96 w-96 rounded-full bg-brand-500/20 blur-3xl" />

      <div className="relative w-full max-w-sm rounded-3xl bg-white p-7 shadow-soft-lg">
        <div className="mb-6 flex flex-col items-center text-center">
          <button
            type="button"
            onClick={onSecretTap}
            aria-label="POS"
            className="mb-3 flex h-14 w-14 items-center justify-center overflow-hidden rounded-2xl bg-brand-50 shadow-soft transition-transform active:scale-95"
          >
            <img
              src="/brand-logo.png"
              alt=""
              className="h-full w-full object-cover"
              onError={(e) => {
                // Falls back to a plain icon if the static asset is ever
                // missing (e.g. a stripped-down build) — never a broken
                // image on the very first screen a cashier sees.
                (e.currentTarget as HTMLImageElement).style.display = "none";
                e.currentTarget.nextElementSibling?.classList.remove("hidden");
              }}
            />
            <Store className="hidden h-6 w-6 text-brand-600" />
          </button>
          <h1 className="text-lg font-semibold text-slate-900">Welcome back</h1>
          <p className="mt-0.5 text-sm text-slate-500">Sign in to start your shift</p>
        </div>

        {loadError && (
          <p className="mb-4 rounded-xl bg-red-50 px-3 py-2 text-sm text-red-700">{loadError}</p>
        )}

        <label className="mb-1.5 block text-xs font-medium text-slate-500" htmlFor="user">
          User
        </label>
        <DropdownSelect
          value={selectedUserId}
          placeholder="Select user"
          onChange={(id) => {
            clearError();
            setPin("");
            setSelectedUserId(id);
          }}
          options={users.map((user) => ({
            value: user.id,
            label: user.name,
            meta: (
              <span className={`rounded-full px-2 py-0.5 text-xs font-semibold capitalize ${ROLE_TONE[user.role]}`}>
                {user.role}
              </span>
            ),
          }))}
        />

        <div className="mt-6 flex justify-center gap-3">
          {Array.from({ length: PIN_MAX_LENGTH }).map((_, index) => (
            <span
              key={index}
              className={[
                "h-3 w-3 rounded-full transition-all duration-150",
                index < pin.length ? "scale-110 bg-brand-600" : "bg-slate-200",
              ].join(" ")}
            />
          ))}
        </div>

        {error && <p className="mt-4 text-center text-sm font-medium text-red-600">{error}</p>}

        <div className="mt-6 grid grid-cols-3 gap-2.5">
          {KEYPAD.map((digit) => (
            <button
              key={digit}
              type="button"
              onClick={() => press(digit)}
              className="rounded-2xl bg-slate-50 py-3.5 text-lg font-semibold text-slate-900 shadow-soft transition-colors hover:bg-slate-100 active:scale-[0.97]"
            >
              {digit}
            </button>
          ))}
          <button
            type="button"
            onClick={() => setPin((c) => c.slice(0, -1))}
            className="flex items-center justify-center rounded-2xl bg-slate-50 py-3.5 text-slate-500 shadow-soft transition-colors hover:bg-slate-100 active:scale-[0.97]"
            aria-label="Delete last digit"
          >
            <Delete className="h-5 w-5" />
          </button>
          <button
            type="button"
            onClick={() => press("0")}
            className="rounded-2xl bg-slate-50 py-3.5 text-lg font-semibold text-slate-900 shadow-soft transition-colors hover:bg-slate-100 active:scale-[0.97]"
          >
            0
          </button>
          <button
            type="button"
            onClick={() => void submit()}
            disabled={isAuthenticating || pin.length < PIN_MIN_LENGTH || selectedUserId === null}
            className="rounded-2xl bg-brand-600 py-3.5 text-sm font-semibold text-white shadow-soft transition-colors hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-40"
          >
            {isAuthenticating ? "…" : "Enter"}
          </button>
        </div>

        <p className="mt-6 text-center text-xs text-slate-400">
          First run? Sign in as Owner with PIN 1234, then change it.
        </p>
      </div>

      {showVendorAccess && (
        <Suspense fallback={null}>
          <ProductOwnerModal onClose={() => setShowVendorAccess(false)} />
        </Suspense>
      )}
    </div>
  );
}
