import { describe, expect, it } from "vitest";
import type { Role } from "../types";
import {
  assignableRoles,
  isAdminRole,
  isModuleReadOnlyFor,
  roleCanAccessModule,
  roleCanAccessSettings,
  roleCanManageAccount,
  roleCanManageUsers,
} from "./permissions";

const ROLES: Role[] = ["owner", "admin", "cashier"];

describe("assignableRoles (mirrors commands::assignable_roles)", () => {
  it("never offers Owner, and offers less the lower the role", () => {
    expect(assignableRoles("owner")).toEqual(["admin", "cashier"]);
    expect(assignableRoles("admin")).toEqual(["cashier"]);
    expect(assignableRoles("cashier")).toEqual([]);
    for (const role of ROLES) expect(assignableRoles(role)).not.toContain("owner");
  });
});

describe("roleCanManageAccount (mirrors commands::caller_may_manage)", () => {
  it("lets only the Owner manage the Owner account, and only their own", () => {
    expect(roleCanManageAccount("owner", "owner", true)).toBe(true);
    expect(roleCanManageAccount("admin", "owner", false)).toBe(false);
    expect(roleCanManageAccount("admin", "owner", true)).toBe(false);
  });

  it("lets an Admin manage cashiers but not other admins", () => {
    expect(roleCanManageAccount("admin", "cashier", false)).toBe(true);
    expect(roleCanManageAccount("admin", "admin", false)).toBe(false);
    expect(roleCanManageAccount("admin", "admin", true)).toBe(true);
  });

  it("never lets a cashier manage anyone else", () => {
    for (const target of ROLES) expect(roleCanManageAccount("cashier", target, false)).toBe(false);
  });
});

describe("role helpers", () => {
  it("treats Owner and Admin as admins", () => {
    expect(ROLES.filter(isAdminRole)).toEqual(["owner", "admin"]);
  });

  it("makes Inventory read-only for a cashier only", () => {
    expect(isModuleReadOnlyFor("cashier", "inventory")).toBe(true);
    expect(isModuleReadOnlyFor("admin", "inventory")).toBe(false);
    expect(isModuleReadOnlyFor("cashier", "billing")).toBe(false);
  });
});

describe("roleCanAccessModule (cashier allowlist)", () => {
  it("lets Owner and Admin open every module", () => {
    for (const role of ["owner", "admin"] as Role[]) {
      expect(roleCanAccessModule(role, "dashboard")).toBe(true);
      expect(roleCanAccessModule(role, "tables")).toBe(true);
      expect(roleCanAccessModule(role, "shifts")).toBe(true);
    }
  });

  it("gives a Cashier the till, stock and the PIN-gated areas only", () => {
    for (const key of ["billing", "inventory", "attendance", "expenses", "salary", "reports", "employees"] as const) {
      expect(roleCanAccessModule("cashier", key)).toBe(true);
    }
    expect(roleCanAccessModule("cashier", "dashboard")).toBe(false);
    expect(roleCanAccessModule("cashier", "settings")).toBe(false);
    expect(roleCanAccessModule("cashier", "users")).toBe(false);
  });
});

describe("Settings and Users screens", () => {
  it("are Owner/Admin only", () => {
    for (const check of [roleCanAccessSettings, roleCanManageUsers]) {
      expect(check("owner")).toBe(true);
      expect(check("admin")).toBe(true);
      expect(check("cashier")).toBe(false);
    }
  });
});
