import { beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "../services/licenseService";
import { useLicenseStore } from "./licenseStore";

vi.mock("../services/licenseService", () => ({
  activateLicense: vi.fn(),
  getLicenseInfo: vi.fn(),
  getLicenseState: vi.fn(),
  onLicenseChanged: vi.fn(),
  refreshLicense: vi.fn(),
}));

const mocked = vi.mocked(service);
const store = () => useLicenseStore.getState();

beforeEach(() => {
  vi.resetAllMocks();
  mocked.onLicenseChanged.mockResolvedValue(() => {});
  mocked.getLicenseInfo.mockResolvedValue(null as never);
  useLicenseStore.setState({ state: null, info: null, isBusy: false, error: null });
});

describe("licenseStore", () => {
  it("treats an unreadable licence state as invalid, i.e. locked", async () => {
    mocked.getLicenseState.mockRejectedValue(new Error("db locked"));
    await store().init();
    expect(store().state).toEqual({ kind: "invalid", message: "db locked" });
  });

  it("subscribes to license:changed only once across repeated init calls", async () => {
    mocked.getLicenseState.mockResolvedValue({ kind: "missing" } as never);
    vi.resetModules(); // the subscription is module state; start from a fresh module
    const { useLicenseStore: fresh } = await import("./licenseStore");
    await fresh.getState().init();
    await fresh.getState().init();
    expect(mocked.onLicenseChanged).toHaveBeenCalledTimes(1);
  });

  it("a failed activation keeps the old state, reports the error and clears busy", async () => {
    useLicenseStore.setState({ state: { kind: "missing" } as never });
    mocked.activateLicense.mockRejectedValue(new Error("Key not recognised"));
    await expect(store().activate("ABCD")).rejects.toThrow("Key not recognised");
    expect(store()).toMatchObject({ state: { kind: "missing" }, error: "Key not recognised", isBusy: false });
  });

  it("a successful refresh stores the new state and returns the offline hint", async () => {
    mocked.refreshLicense.mockResolvedValue({ state: { kind: "missing" }, hint: "offline" } as never);
    await expect(store().refresh()).resolves.toBe("offline");
    expect(store()).toMatchObject({ state: { kind: "missing" }, error: null, isBusy: false });
  });
});
