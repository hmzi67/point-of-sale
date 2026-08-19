import { LogOut, Menu } from "lucide-react";
import { useAppStore, useAuthStore } from "../../store";

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
  const user = useAuthStore((state) => state.user);
  const logout = useAuthStore((state) => state.logout);

  return (
    <header className="flex h-14 shrink-0 items-center justify-between border-b border-slate-100 bg-white px-4">
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onOpenNav}
          aria-label="Open navigation"
          className="rounded-xl p-2 text-slate-500 hover:bg-slate-100 hover:text-slate-900"
        >
          <Menu className="h-5 w-5" />
        </button>
        <h1 className="text-base font-semibold text-slate-900">{businessName}</h1>
      </div>

      <div className="flex items-center gap-4 text-sm">
        <span className="hidden items-center gap-1.5 text-slate-500 sm:inline-flex">
          <span className="h-2 w-2 rounded-full bg-emerald-500" />
          Offline ready
        </span>

        {user && (
          <>
            <span className="text-slate-700">
              {user.name}
              <span className="ml-1.5 rounded bg-slate-100 px-1.5 py-0.5 text-xs text-slate-500">
                {ROLE_LABEL[user.role] ?? user.role}
              </span>
            </span>
            <button
              type="button"
              onClick={logout}
              className="inline-flex items-center gap-1.5 rounded-xl px-2 py-1 text-slate-500 hover:bg-slate-100 hover:text-slate-900"
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
