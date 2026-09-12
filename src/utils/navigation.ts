import {
  Boxes,
  CalendarCheck,
  Clock,
  IdCard,
  LayoutDashboard,
  Receipt,
  Settings,
  ShoppingCart,
  Table2,
  TrendingUp,
  Users,
  Wallet,
  type LucideIcon,
} from "lucide-react";
import type { ModuleKey } from "../types";

export interface NavItem {
  key: ModuleKey;
  path: string;
  icon: LucideIcon;
}

/**
 * Presentation only: the route and icon for each module key. Labels and
 * ordering come from the `modules` table, and visibility from
 * `enabled_modules` — nothing here decides whether an item is shown.
 */
export const MODULE_NAV: Record<ModuleKey, NavItem> = {
  dashboard: { key: "dashboard", path: "/", icon: LayoutDashboard },
  billing: { key: "billing", path: "/billing", icon: ShoppingCart },
  inventory: { key: "inventory", path: "/inventory", icon: Boxes },
  reports: { key: "reports", path: "/reports", icon: TrendingUp },
  tables: { key: "tables", path: "/tables", icon: Table2 },
  attendance: { key: "attendance", path: "/attendance", icon: CalendarCheck },
  expenses: { key: "expenses", path: "/expenses", icon: Receipt },
  salary: { key: "salary", path: "/salary", icon: Wallet },
  shifts: { key: "shifts", path: "/shifts", icon: Clock },
  employees: { key: "employees", path: "/employees", icon: IdCard },
  users: { key: "users", path: "/users", icon: Users },
  settings: { key: "settings", path: "/settings", icon: Settings },
};

export function pathForModule(key: ModuleKey): string {
  return MODULE_NAV[key]?.path ?? "/";
}
