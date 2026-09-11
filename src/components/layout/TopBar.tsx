import { lazy, Suspense, useState } from "react";
import { LogOut, Menu, Moon, Sun } from "lucide-react";
import { useLogoDataUrl } from "../../hooks/useLogoDataUrl";
import { useSecretTapTrigger } from "../../hooks/useSecretTapTrigger";
import { useAppStore, useAuthStore, useThemeStore } from "../../store";

// Lazy so this never loads for the overwhelming majority of app sessions
// that never trigger it — see `useSecretTapTrigger`'s doc comment.
const ProductOwnerModal = lazy(() =>
  import("../productOwner/ProductOwnerModal").then((m) => ({ default: m.ProductOwnerModal })),
);

const ROLE_LABEL: Record<string, string> = {
  owner: "Owner",
  admin: "Admin",
  cashier: "Cashier",
};

interface TopBarProps {
  onOpenNav: () => void;
}

export function TopBar({ onOpenNav }: TopBarProps) {
  const businessName = useAppStore((state) => state.config.businessName);
  const logoPath = useAppStore((state) => state.config.logoPath);
  const logoDataUrl = useLogoDataUrl(logoPath);
  const user = useAuthStore((state) => state.user);
  const logout = useAuthStore((state) => state.logout);
  const theme = useThemeStore((state) => state.theme);
  const toggleTheme = useThemeStore((state) => state.toggle);

  const [showVendorAccess, setShowVendorAccess] = useState(false);
  const onSecretTap = useSecretTapTrigger(() => setShowVendorAccess(true));

  return (
    // min-h (not h-) + pt-[env(safe-area-inset-top)]: on Android the WebView
    // draws edge-to-edge behind the status bar (MainActivity calls
    // enableEdgeToEdge()), so without this the bar's own 56px content row
    // renders partially under the status bar/notch instead of below it. A
    // fixed height would clip that padding instead of growing to fit it.
    <header className="flex min-h-14 shrink-0 items-center justify-between border-b border-line bg-surface px-4 pt-[env(safe-area-inset-top)]">
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onOpenNav}
          aria-label="Open navigation"
          // min-h/min-w-11 (44px): the visual icon+padding box otherwise
          // comes out around 36px, under the ~44px minimum comfortable touch
          // target on a phone.
          className="flex min-h-11 min-w-11 items-center justify-center rounded-xl p-2 text-fg-muted hover:bg-surface-3 hover:text-fg"
        >
          <Menu className="h-5 w-5" />
        </button>
        {logoDataUrl && <img src={logoDataUrl} alt="" className="h-7 w-7 rounded object-contain" />}
        <h1 className="text-base font-semibold text-fg" onClick={onSecretTap}>
          {businessName}
        </h1>
      </div>

      {showVendorAccess && (
        <Suspense fallback={null}>
          <ProductOwnerModal onClose={() => setShowVendorAccess(false)} />
        </Suspense>
      )}

      <div className="flex items-center gap-4 text-sm">
        <span className="hidden items-center gap-1.5 text-fg-muted sm:inline-flex">
          <span className="h-2 w-2 rounded-full bg-success" />
          Offline ready
        </span>

        {/* Quick theme flip — the same preference Settings → Appearance
            sets, just one tap away for a cashier mid-shift. Icon shows
            what you'd switch *to*. */}
        <button
          type="button"
          onClick={() => void toggleTheme()}
          aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
          title={theme === "dark" ? "Light theme" : "Dark theme"}
          className="flex min-h-11 min-w-11 items-center justify-center rounded-xl p-2 text-fg-muted hover:bg-surface-3 hover:text-fg"
        >
          {theme === "dark" ? <Sun className="h-4.5 w-4.5" /> : <Moon className="h-4.5 w-4.5" />}
        </button>

        {user && (
          <>
            <span className="text-fg-2">
              {user.name}
              <span className="ml-1.5 rounded bg-surface-3 px-1.5 py-0.5 text-xs text-fg-muted">
                {ROLE_LABEL[user.role] ?? user.role}
              </span>
            </span>
            <button
              type="button"
              onClick={logout}
              className="inline-flex min-h-11 items-center gap-1.5 rounded-xl px-2.5 py-1 text-fg-muted hover:bg-surface-3 hover:text-fg"
            >
              <LogOut className="h-4 w-4" />
              Sign out
            </button>
          </>
        )}
      </div>
    </header>
  );
}
