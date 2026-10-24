import { beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "../services/workspaceService";

vi.mock("../services/workspaceService", () => ({
  getWorkspaceInfo: vi.fn(),
  onWorkspaceApplied: vi.fn(),
  onWorkspaceReplaced: vi.fn(),
  onWorkspaceStatus: vi.fn(),
  syncWorkspaceNow: vi.fn(),
}));

const mocked = vi.mocked(service);

beforeEach(() => {
  vi.resetAllMocks();
  vi.resetModules(); // the subscription is module state
  mocked.onWorkspaceStatus.mockResolvedValue(() => {});
  mocked.onWorkspaceApplied.mockResolvedValue(() => {});
  mocked.onWorkspaceReplaced.mockResolvedValue(() => {});
});

describe("workspaceStore", () => {
  it("subscribes once across repeated init calls", async () => {
    mocked.getWorkspaceInfo.mockResolvedValue({ mode: "standalone" } as never);
    const { useWorkspaceStore } = await import("./workspaceStore");
    await useWorkspaceStore.getState().init();
    await useWorkspaceStore.getState().init();
    expect(mocked.onWorkspaceStatus).toHaveBeenCalledTimes(1);
    expect(useWorkspaceStore.getState().info).toEqual({ mode: "standalone" });
  });

  it("keeps the last info when it cannot be read (signed out)", async () => {
    const { useWorkspaceStore } = await import("./workspaceStore");
    useWorkspaceStore.setState({ info: { mode: "connected" } as never });
    mocked.getWorkspaceInfo.mockRejectedValue(new Error("Not signed in"));
    await useWorkspaceStore.getState().refresh();
    expect(useWorkspaceStore.getState().info).toEqual({ mode: "connected" });
  });

  it("signs out when this counter's data is replaced by the business data", async () => {
    let replaced: () => void = () => {};
    mocked.onWorkspaceReplaced.mockImplementation(async (handler) => {
      replaced = handler as () => void;
      return () => {};
    });
    mocked.getWorkspaceInfo.mockResolvedValue({ mode: "connected" } as never);
    const { useWorkspaceStore } = await import("./workspaceStore");
    const { useAuthStore } = await import("./authStore");
    const logout = vi.fn();
    useAuthStore.setState({ logout });
    // The handler also tells open screens to reload (a window event); tests run without a DOM.
    vi.stubGlobal("window", new EventTarget());
    await useWorkspaceStore.getState().init();
    replaced();
    expect(logout).toHaveBeenCalledTimes(1);
    vi.unstubAllGlobals();
  });
});
