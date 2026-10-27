import { lazy, Suspense, useEffect, useState } from "react";
import { Navigate } from "react-router-dom";
import { ArrowRight, Delete, Keyboard, ShieldCheck, Store, WifiOff, Zap } from "lucide-react";
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

/** Above this many accounts the staff tiles would crowd the card, so the
 * picker falls back to the dropdown. */
const MAX_USER_TILES = 6;

/** Role tint — same "tone conveys meaning at a glance" idea the rest of the
 * app uses for status pills, so Owner/Admin/Cashier read apart instantly. */
const ROLE_TONE: Record<Role, string> = {
  owner: "bg-warning-soft text-warning-text",
  admin: "bg-accent-soft text-accent-text",
  cashier: "bg-surface-3 text-fg-2",
  delivery: "bg-success-soft text-success-text",
};

function initials(name: string) {
  const parts = name.trim().split(/\s+/);
  return ((parts[0]?.[0] ?? "") + (parts.length > 1 ? parts[parts.length - 1][0] : "")).toUpperCase() || "?";
}

/** Ticks once a minute, on the minute — enough for an HH:MM clock. */
function useNow() {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    let interval: number | undefined;
    const timeout = window.setTimeout(() => {
      setNow(new Date());
      interval = window.setInterval(() => setNow(new Date()), 60_000);
    }, 60_000 - (Date.now() % 60_000));
    return () => {
      window.clearTimeout(timeout);
      window.clearInterval(interval);
    };
  }, []);
  return now;
}

/** The logo doubles as the hidden vendor trigger (7 taps). Falls back to a
 * plain icon if the static asset is ever missing — never a broken image on
 * the very first screen a cashier sees. */
function BrandMark({ onTap, className }: { onTap: () => void; className: string }) {
  return (
    <button
      type="button"
      onClick={onTap}
      aria-label="Diwan"
      className={`flex shrink-0 items-center justify-center overflow-hidden transition-transform active:scale-95 ${className}`}
    >
      <img
        src="/brand-logo.png"
        alt=""
        className="h-full w-full object-cover"
        onError={(e) => {
          (e.currentTarget as HTMLImageElement).style.display = "none";
          e.currentTarget.nextElementSibling?.classList.remove("hidden");
        }}
      />
      <Store className="hidden h-6 w-6 text-accent-text" />
    </button>
  );
}

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
  // Bumped on each failed attempt so the dots remount and shake again.
  const [shake, setShake] = useState(0);
  const now = useNow();

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

  useEffect(() => {
    if (error) setShake((n) => n + 1);
  }, [error]);

  const selectedUser = users.find((user) => user.id === selectedUserId) ?? null;
  const canSubmit = !isAuthenticating && pin.length >= PIN_MIN_LENGTH && selectedUserId !== null;

  const submit = async () => {
    if (selectedUserId === null || pin.length < PIN_MIN_LENGTH) return;
    await login(selectedUserId, pin);
    setPin("");
  };

  const press = (digit: string) => {
    clearError();
    setPin((current) => (current.length >= PIN_MAX_LENGTH ? current : current + digit));
  };

  const chooseUser = (id: number | null) => {
    clearError();
    setPin("");
    setSelectedUserId(id);
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

  const keyClass =
    "flex h-14 items-center justify-center rounded-2xl border border-line bg-surface-2 text-xl font-semibold text-fg " +
    "shadow-[0_2px_0_0_var(--color-line)] transition-all hover:bg-surface-3 active:translate-y-[2px] active:shadow-none";

  return (
    <div className="login-layout flex h-full overflow-hidden bg-bg">
      {/* ===== Brand panel (wide windows) ===== */}
      <aside className="login-brand relative hidden w-[44%] max-w-[560px] flex-col justify-between overflow-hidden p-10 lg:flex">
        <div className="login-brand__grid pointer-events-none absolute inset-0" />

        <div className="relative flex items-center gap-3">
          <BrandMark onTap={onSecretTap} className="h-11 w-11 rounded-xl shadow-[0_0_0_1px_rgba(0,229,160,0.35),0_10px_30px_-10px_rgba(0,229,160,0.6)]" />
          <div>
            <p className="text-lg font-semibold tracking-tight">Diwan</p>
            <p className="login-brand__muted text-[11px] font-semibold uppercase tracking-[0.22em]">Point of Sale</p>
          </div>
        </div>

        <div className="relative">
          <p className="text-[clamp(3.5rem,6vw,5rem)] font-semibold leading-none tracking-tight tabular-nums">
            {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </p>
          <p className="login-brand__muted mt-3 text-base">
            {now.toLocaleDateString([], { weekday: "long", day: "numeric", month: "long", year: "numeric" })}
          </p>
          <p className="mt-10 max-w-xs text-2xl font-semibold leading-snug tracking-tight">
            Ring up every sale at the speed of typing.
          </p>
        </div>

        <ul className="relative flex flex-wrap gap-2 text-xs font-medium">
          <li className="login-brand__chip flex items-center gap-1.5 rounded-full px-3 py-1.5">
            <WifiOff className="h-3.5 w-3.5" /> Works offline
          </li>
          <li className="login-brand__chip flex items-center gap-1.5 rounded-full px-3 py-1.5">
            <Keyboard className="h-3.5 w-3.5" /> Keyboard-first
          </li>
          <li className="login-brand__chip flex items-center gap-1.5 rounded-full px-3 py-1.5">
            <ShieldCheck className="h-3.5 w-3.5" /> PIN-protected shifts
          </li>
        </ul>
      </aside>

      {/* ===== Sign-in ===== */}
      <main className="login-main relative flex min-w-0 flex-1 overflow-x-hidden overflow-y-auto p-6">
        {/* Decorative glows, in the theme's accent tint. */}
        <div className="pointer-events-none absolute inset-0 overflow-hidden" aria-hidden="true">
          <div className="absolute -right-24 -top-24 h-80 w-80 rounded-full bg-accent-soft blur-3xl" />
          <div className="absolute -bottom-32 left-10 h-72 w-72 rounded-full bg-accent-soft opacity-60 blur-3xl" />
        </div>

        {/* m-auto, not items-center: a short window scrolls instead of clipping the top. */}
        <div className="relative m-auto w-full max-w-[400px]">
          {/* Compact brand header when the panel is hidden */}
          <div className="mb-6 flex items-center justify-center gap-3 lg:hidden">
            <BrandMark onTap={onSecretTap} className="h-11 w-11 rounded-xl bg-accent-soft shadow-soft" />
            <div>
              <p className="text-lg font-semibold tracking-tight text-fg">Diwan</p>
              <p className="text-xs tabular-nums text-fg-muted">
                {now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })} ·{" "}
                {now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" })}
              </p>
            </div>
          </div>

          <div className="login-card rounded-3xl border border-line bg-surface p-7 shadow-soft-lg">
            <h1 className="text-2xl font-semibold tracking-tight text-fg">Welcome back</h1>
            <p className="mt-1 text-sm text-fg-muted">Pick your name, then enter your PIN.</p>

            {loadError && (
              <p className="mt-4 rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{loadError}</p>
            )}

            {/* Who is signing in */}
            <div className="mt-6">
              <p className="mb-2 text-xs font-semibold uppercase tracking-wider text-fg-faint">Staff</p>
              {users.length > 0 && users.length <= MAX_USER_TILES ? (
                <div className="grid grid-cols-2 gap-2" role="radiogroup" aria-label="Staff">
                  {users.map((user) => {
                    const active = user.id === selectedUserId;
                    return (
                      <button
                        key={user.id}
                        type="button"
                        role="radio"
                        aria-checked={active}
                        onClick={() => chooseUser(user.id)}
                        className={[
                          "flex min-w-0 items-center gap-2.5 rounded-2xl border p-2.5 text-left transition-colors",
                          active
                            ? "border-accent bg-accent-soft ring-1 ring-accent"
                            : "border-line bg-surface-2 hover:border-line-strong hover:bg-surface-3",
                          users.length === 1 ? "col-span-2" : "",
                        ].join(" ")}
                      >
                        <span
                          className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-xl text-sm font-bold ${ROLE_TONE[user.role]}`}
                        >
                          {initials(user.name)}
                        </span>
                        <span className="min-w-0">
                          <span className="block truncate text-sm font-semibold text-fg">{user.name}</span>
                          {user.name.trim().toLowerCase() !== user.role && (
                            <span className="block text-xs capitalize text-fg-muted">{user.role}</span>
                          )}
                        </span>
                      </button>
                    );
                  })}
                </div>
              ) : (
                <DropdownSelect
                  value={selectedUserId}
                  placeholder={users.length ? "Select user" : "Loading…"}
                  onChange={chooseUser}
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
              )}
            </div>

            {/* PIN */}
            <div className="mt-6 text-center">
              <p className="text-sm text-fg-muted">
                Enter PIN{selectedUser ? <> for <span className="font-semibold text-fg">{selectedUser.name}</span></> : ""}
              </p>
              <div
                key={shake}
                className={`mt-3 flex justify-center gap-3.5 ${shake ? "animate-[pin-shake_0.4s_ease-in-out]" : ""}`}
                aria-label={`${pin.length} of ${PIN_MAX_LENGTH} digits entered`}
                role="status"
              >
                {Array.from({ length: PIN_MAX_LENGTH }).map((_, index) => {
                  const filled = index < pin.length;
                  return (
                    <span
                      key={index}
                      className={[
                        "h-3.5 w-3.5 rounded-full border-2 transition-all duration-150",
                        error
                          ? "border-danger bg-danger"
                          : filled
                            ? "scale-110 border-accent bg-accent"
                            : "border-line-strong bg-transparent",
                      ].join(" ")}
                    />
                  );
                })}
              </div>
              <p className="mt-3 min-h-5 text-sm font-medium text-danger-text" aria-live="polite">
                {error ?? ""}
              </p>
            </div>

            {/* Keypad */}
            <div className="mt-2 grid grid-cols-3 gap-2.5">
              {KEYPAD.map((digit) => (
                <button key={digit} type="button" onClick={() => press(digit)} className={keyClass}>
                  {digit}
                </button>
              ))}
              <button
                type="button"
                onClick={() => setPin((c) => c.slice(0, -1))}
                disabled={pin.length === 0}
                className={`${keyClass} text-fg-muted disabled:opacity-40`}
                aria-label="Delete last digit"
              >
                <Delete className="h-5 w-5" />
              </button>
              <button type="button" onClick={() => press("0")} className={keyClass}>
                0
              </button>
              <button
                type="button"
                onClick={() => void submit()}
                disabled={!canSubmit}
                className="flex h-14 items-center justify-center gap-1.5 rounded-2xl bg-accent text-sm font-semibold text-accent-ink shadow-[0_8px_24px_-10px_var(--color-accent)] transition-all hover:bg-accent-hover active:translate-y-[2px] disabled:cursor-not-allowed disabled:opacity-40 disabled:shadow-none"
              >
                {isAuthenticating ? (
                  <span className="h-4 w-4 animate-spin rounded-full border-2 border-current border-t-transparent" aria-label="Signing in" />
                ) : (
                  <>
                    Enter <ArrowRight className="h-4 w-4" />
                  </>
                )}
              </button>
            </div>

            <p className="mt-5 flex items-center justify-center gap-1.5 text-xs text-fg-faint">
              <Zap className="h-3.5 w-3.5" /> Tip: type your PIN on the keyboard and press Enter.
            </p>
          </div>

          <p className="mt-4 text-center text-xs text-fg-muted">
            First run? Sign in as Owner with PIN 1234, then change it.
          </p>
        </div>
      </main>

      {showVendorAccess && (
        <Suspense fallback={null}>
          <ProductOwnerModal onClose={() => setShowVendorAccess(false)} />
        </Suspense>
      )}
    </div>
  );
}
