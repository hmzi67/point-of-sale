import { beforeEach, describe, expect, it, vi } from "vitest";
import * as config from "../services/configService";
import * as log from "../services/logService";
import { useUiPrefsStore } from "./uiPrefsStore";

vi.mock("../services/configService", () => ({ getUiPrefs: vi.fn(), setSidebarCollapsed: vi.fn(), setTheme: vi.fn() }));
vi.mock("../services/logService", () => ({ devWarn: vi.fn(), reportWarning: vi.fn() }));
vi.mock("../services/platform", () => ({ IS_ANDROID: false }));

const mocked = vi.mocked(config);
const store = () => useUiPrefsStore.getState();

beforeEach(() => {
  vi.resetAllMocks();
  useUiPrefsStore.setState({ theme: "dark", sidebarCollapsed: false });
});

describe("uiPrefsStore", () => {
  it("toggles the theme and persists it", async () => {
    mocked.setTheme.mockResolvedValue(undefined);
    await store().toggleTheme();
    expect(store().theme).toBe("light");
    expect(mocked.setTheme).toHaveBeenCalledWith("light");
  });

  it("rolls the theme back and logs when it can't be saved", async () => {
    mocked.setTheme.mockRejectedValue(new Error("read-only db"));
    await store().setTheme("light");
    expect(store().theme).toBe("dark");
    expect(log.reportWarning).toHaveBeenCalledTimes(1);
  });

  it("rolls the sidebar back when it can't be saved", async () => {
    mocked.setSidebarCollapsed.mockRejectedValue(new Error("nope"));
    await store().toggleSidebar();
    expect(store().sidebarCollapsed).toBe(false);
  });

  it("keeps defaults when the stored prefs can't be read", async () => {
    mocked.getUiPrefs.mockRejectedValue(new Error("no db"));
    await store().init();
    expect(store().theme).toBe("dark");
  });
});
