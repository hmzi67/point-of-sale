import { beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "../services/moduleService";
import type { ModuleState } from "../types";
import { useModuleStore } from "./moduleStore";

vi.mock("../services/moduleService", () => ({ getEnabledModules: vi.fn(), toggleModule: vi.fn() }));

const mocked = vi.mocked(service);
const store = () => useModuleStore.getState();
const billing = { key: "billing", enabled: true } as ModuleState;

beforeEach(() => {
  vi.resetAllMocks();
  useModuleStore.setState({ modules: [], isLoaded: false, isLoading: false, error: null });
});

describe("moduleStore", () => {
  it("takes the toggle command's refreshed list as the new state", async () => {
    mocked.toggleModule.mockResolvedValue([{ ...billing, enabled: false }]);
    await store().toggle("billing", false);
    expect(store().modules).toEqual([{ ...billing, enabled: false }]);
  });

  it("keeps the old list and rethrows when a toggle is refused", async () => {
    useModuleStore.setState({ modules: [billing] });
    mocked.toggleModule.mockRejectedValue(new Error("locked by the vendor"));
    await expect(store().toggle("billing", false)).rejects.toThrow("locked");
    expect(store()).toMatchObject({ modules: [billing], error: "locked by the vendor" });
  });

  it("marks itself loaded even when the read fails, so guards don't wait forever", async () => {
    mocked.getEnabledModules.mockRejectedValue(new Error("no db"));
    await store().load();
    expect(store()).toMatchObject({ isLoaded: true, isLoading: false, error: "no db" });
  });
});
