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
};

/** Settings is not a module — it is the screen that configures them. */
export const SETTINGS_NAV = { path: "/settings", label: "Settings", icon: Settings };

/** Same tier as Settings (admin-only, always present when the role allows
 * it) — not a module either, so it isn't in `MODULE_NAV`/`enabled_modules`. */
export const USERS_NAV = { path: "/users", label: "Users", icon: Users };

/** Same tier again — employee (payroll/attendance) records, not login
 * accounts. Not a module either: Attendance and Salary both read this list
 * regardless of whether either module is toggled on, so it isn't gated by
 * `enabled_modules`. */
export const EMPLOYEES_NAV = { path: "/employees", label: "Employees", icon: IdCard };

export function pathForModule(key: ModuleKey): string {
  return MODULE_NAV[key]?.path ?? "/";
}
