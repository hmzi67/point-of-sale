import { describe, expect, it } from "vitest";
import { MODULE_NAV, pathForModule } from "./navigation";

describe("navigation", () => {
  it("gives every module a route, and the dashboard the root", () => {
    expect(pathForModule("dashboard")).toBe("/");
    expect(pathForModule("customers")).toBe("/customers");
    for (const [key, nav] of Object.entries(MODULE_NAV)) {
      expect(nav.key).toBe(key);
      expect(nav.path.startsWith("/")).toBe(true);
    }
  });

  it("falls back to the root for an unknown key", () => {
    expect(pathForModule("nope" as never)).toBe("/");
  });
});
