import { Link } from "react-router-dom";
import { ArrowUpRight, Zap } from "lucide-react";
import type { VisibleModule } from "../../hooks/useModules";

const DESCRIPTIONS: Record<string, string> = {
  billing: "Create & manage orders",
  inventory: "Products & stock levels",
  reports: "Explore your numbers",
  tables: "Manage your floor",
  attendance: "Track staff attendance",
  expenses: "Record business costs",
  salary: "Manage staff payments",
  shifts: "Open & close shifts",
  customers: "Accounts, credit & installments",
  purchasing: "Suppliers, orders & goods received",
  ledger: "Receivables & payables",
};

/** Links respect the same enabled modules and role permissions as navigation. */
export function QuickLinks({ modules }: { modules: VisibleModule[] }) {
  const links = modules.filter((m) => m.key !== "dashboard");
  if (links.length === 0) return null;

  return (
    <div className="rounded-2xl border border-line bg-surface p-5">
      <div className="flex items-center gap-2">
        <Zap className="h-4 w-4 text-accent-text" />
        <h3 className="text-sm font-semibold text-fg">Quick access</h3>
      </div>
      <p className="mt-1.5 text-xs text-fg-muted">Your everyday tools, one tap away.</p>
      <div className="mt-5 grid grid-cols-1 gap-2.5 sm:grid-cols-2 xl:grid-cols-1">
        {links.map((module) => {
          const Icon = module.nav.icon;
          return (
            <Link key={module.key} to={module.nav.path}
              className="group flex items-center gap-3 rounded-xl border border-line/70 bg-surface-2/40 p-3 transition-colors hover:border-accent/40 hover:bg-accent-soft focus-visible:outline-2 focus-visible:outline-accent">
              <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border border-line bg-surface text-fg-muted transition-colors group-hover:border-accent/20 group-hover:text-accent-text">
                <Icon className="h-4 w-4" strokeWidth={1.75} />
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-xs font-semibold text-fg">{module.name}</span>
                <span className="mt-1 block text-[11px] text-fg-muted">{DESCRIPTIONS[module.key] ?? `Open ${module.name.toLowerCase()}`}</span>
              </span>
              <ArrowUpRight className="h-4 w-4 shrink-0 text-fg-muted transition-colors group-hover:text-accent-text" />
            </Link>
          );
        })}
      </div>
    </div>
  );
}
