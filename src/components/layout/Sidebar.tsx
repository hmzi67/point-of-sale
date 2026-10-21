import { NavLink } from "react-router-dom";
import { LogOut, PanelLeftClose, PanelLeftOpen, User } from "lucide-react";
import { useLogoDataUrl } from "../../hooks/useLogoDataUrl";
import { useNavItems } from "../../hooks/useNavItems";
import { useAppStore, useAuthStore, useUiPrefsStore } from "../../store";

const ROLE_LABEL: Record<string, string> = {
  owner: "Owner",
  admin: "Admin",
  cashier: "Cashier",
};

/** Expanded / collapsed rail widths. Both are literal so the `w-*` classes
 * below are static (Tailwind can't see a computed class) and the `<main>`
 * beside it simply takes the rest via `flex-1`. */
const WIDTH_EXPANDED = "w-60";
const WIDTH_COLLAPSED = "w-16";

/**
 * Desktop navigation: an always-attached left rail that pushes the content
 * rather than overlaying it (Android keeps its hamburger + `NavOverlay`
 * instead — see `AppLayout`). Renders `useNavItems()`, the same
 * role-filtered list the overlay does, so both platforms agree on what a
 * role can see.
 *
 * Collapsed, it is icon-only: labels come back as native `title` tooltips
 * and the row centres its icon. The width animates rather than jumping;
 * labels are hidden outright (not faded) so a half-clipped word never
 * shows mid-transition. The preference persists to `app_config` via
 * `useUiPrefsStore`, next to the theme.
 */
export function Sidebar() {
  const { items, isLoaded } = useNavItems();
  const user = useAuthStore((state) => state.user);
  const logout = useAuthStore((state) => state.logout);
  const businessName = useAppStore((state) => state.config.businessName);
  const logoPath = useAppStore((state) => state.config.logoPath);
  const logoDataUrl = useLogoDataUrl(logoPath);
  const collapsed = useUiPrefsStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useUiPrefsStore((state) => state.toggleSidebar);

  const rowClass = (extra: string) =>
    [
      "flex min-h-10 items-center rounded-xl text-sm font-medium transition-colors",
      collapsed ? "justify-center px-0" : "gap-3 px-3",
      extra,
    ].join(" ");

  const linkClass = ({ isActive }: { isActive: boolean }) =>
    rowClass(isActive ? "bg-accent text-accent-ink shadow-soft" : "text-fg-2 hover:bg-surface-3 hover:text-fg");

  return (
    <aside
      className={[
        "flex h-full shrink-0 flex-col border-r border-line bg-surface glass transition-[width] duration-200 ease-out",
        collapsed ? WIDTH_COLLAPSED : WIDTH_EXPANDED,
      ].join(" ")}
    >
      {/* Brand row + collapse toggle. The toggle sits at the far edge when
          expanded and is the only thing in the row when collapsed, so it's
          always in the same place relative to the rail's right edge. */}
      <div className={["flex h-14 shrink-0 items-center border-b border-line", collapsed ? "justify-center" : "gap-2 px-3"].join(" ")}>
        {!collapsed && (
          <>
            {logoDataUrl ? (
              <img src={logoDataUrl} alt="" className="h-8 w-8 shrink-0 rounded-lg object-contain" />
            ) : (
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-accent text-sm font-bold text-accent-ink">
                {businessName.charAt(0).toUpperCase()}
              </span>
            )}
            <span className="min-w-0 flex-1 truncate text-sm font-semibold text-fg">{businessName}</span>
          </>
        )}
        <button
          type="button"
          onClick={() => void toggleSidebar()}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!collapsed}
          title={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-fg-muted hover:bg-surface-3 hover:text-fg"
        >
          {collapsed ? <PanelLeftOpen className="h-4.5 w-4.5" /> : <PanelLeftClose className="h-4.5 w-4.5" />}
        </button>
      </div>

      <nav className="flex-1 overflow-y-auto overflow-x-hidden p-2">
        {!isLoaded ? (
          !collapsed && <p className="px-3 py-2 text-sm text-fg-muted">Loading…</p>
        ) : (
          <ul className="space-y-1">
            {items.map(({ key, path, label, icon: Icon, end }) => (
              <li key={key}>
                <NavLink to={path} end={end} className={linkClass} title={collapsed ? label : undefined}>
                  <Icon className="h-4.5 w-4.5 shrink-0" />
                  {!collapsed && <span className="truncate">{label}</span>}
                </NavLink>
              </li>
            ))}
          </ul>
        )}
      </nav>

      <div className="shrink-0 space-y-1 border-t border-line p-2">
        {user && (
          <div className={rowClass("text-fg")} title={collapsed ? `${user.name} · ${ROLE_LABEL[user.role] ?? user.role}` : undefined}>
            <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-accent-soft text-accent-text">
              <User className="h-4 w-4" />
            </span>
            {!collapsed && (
              <span className="min-w-0">
                <span className="block truncate text-sm font-semibold text-fg">{user.name}</span>
                <span className="block text-xs text-fg-muted">{ROLE_LABEL[user.role] ?? user.role}</span>
              </span>
            )}
          </div>
        )}
        <button
          type="button"
          onClick={logout}
          title={collapsed ? "Log Out" : undefined}
          className={rowClass("w-full text-danger-text hover:bg-danger-soft")}
        >
          <LogOut className="h-4.5 w-4.5 shrink-0" />
          {!collapsed && "Log Out"}
        </button>
      </div>
    </aside>
  );
}
