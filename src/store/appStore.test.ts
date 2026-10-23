import { beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "../services/configService";
import type { AppConfig } from "../types";
import { useAppStore } from "./appStore";

vi.mock("../services/configService", () => ({ getAppConfig: vi.fn(), updateAppConfig: vi.fn(), uploadLogo: vi.fn() }));

const mocked = vi.mocked(service);
const store = () => useAppStore.getState();

beforeEach(() => {
  vi.resetAllMocks();
});

describe("appStore", () => {
  it("starts with a placeholder that never triggers onboarding", () => {
    expect(store().config.onboardingCompleted).toBe(true);
  });

  it("saves a patch and adopts the config Rust returns", async () => {
    mocked.updateAppConfig.mockResolvedValue({ businessName: "Chai Spot" } as AppConfig);
    await store().save({ businessName: "Chai Spot" });
    expect(mocked.updateAppConfig).toHaveBeenCalledWith({ businessName: "Chai Spot" });
    expect(store().config.businessName).toBe("Chai Spot");
  });

  it("rethrows a refused save so the form can show it", async () => {
    mocked.updateAppConfig.mockRejectedValue(new Error("Tax percent must be between 0 and 100"));
    await expect(store().save({ taxPercent: 120 })).rejects.toThrow("Tax percent");
    expect(store().error).toBe("Tax percent must be between 0 and 100");
  });
});
