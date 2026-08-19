import { Link } from "react-router-dom";
import { ChevronRight } from "lucide-react";
import type { VisibleModule } from "../../hooks/useModules";

interface QuickLinksProps {
  modules: VisibleModule[];
}

/**
 * One card per module the signed-in user can currently see —
 * `useModules().visibleModules` already intersects "enabled for this
 * installation" with "permitted for this role", so a client with Table
 * Management switched off (or a cashier, if this were ever shown to one)
 * simply never gets a card for it. No placeholder, no disabled-looking
 * ghost card — the card just doesn't exist, same as the sidebar.
 */
export function QuickLinks({ modules }: QuickLinksProps) {
  const links = modules.filter((m) => m.key !== "dashboard");

  if (links.length === 0) return null;

  return (
    <div>
      <h3 className="text-sm font-semibold text-slate-900">Go to</h3>
      <div className="mt-2 grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
        {links.map((module) => {
          const Icon = module.nav.icon;
          return (
            <Link
              key={module.key}
              to={module.nav.path}
              className="flex items-center justify-between gap-2 rounded-lg border border-slate-200 bg-white p-4 transition-colors hover:border-brand-300 hover:bg-brand-50"
            >
              <span className="flex items-center gap-2 text-sm font-medium text-slate-900">
                <Icon className="h-4 w-4 text-slate-500" />
                {module.name}
              </span>
              <ChevronRight className="h-4 w-4 text-slate-400" />
            </Link>
          );
        })}
      </div>
    </div>
  );
}
