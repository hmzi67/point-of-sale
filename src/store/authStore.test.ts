import { beforeEach, describe, expect, it, vi } from "vitest";
import * as auth from "../services/authService";
import * as log from "../services/logService";
import type { User } from "../types";
import { useAuthStore } from "./authStore";

vi.mock("../services/authService", () => ({ login: vi.fn(), logout: vi.fn() }));
vi.mock("../services/logService", () => ({ reportWarning: vi.fn() }));

const mocked = vi.mocked(auth);
const store = () => useAuthStore.getState();
const owner = { id: 1, name: "Owner", role: "owner" } as User;

beforeEach(() => {
  vi.resetAllMocks();
  useAuthStore.setState({ user: null, isAuthenticating: false, error: null });
});

describe("authStore", () => {
  it("signs in, and a wrong PIN leaves nobody signed in", async () => {
    mocked.login.mockResolvedValue(owner);
    await store().login(1, "1234");
    expect(store().user).toEqual(owner);

    mocked.login.mockRejectedValue(new Error("Incorrect PIN"));
    await store().login(1, "0000");
    expect(store()).toMatchObject({ user: null, error: "Incorrect PIN", isAuthenticating: false });
  });

  it("signs out locally at once, retries the backend once, then logs a warning", async () => {
    useAuthStore.setState({ user: owner });
    mocked.logout.mockRejectedValue(new Error("ipc down"));
    store().logout();
    expect(store().user).toBeNull();
    await vi.waitFor(() => expect(log.reportWarning).toHaveBeenCalledTimes(1));
    expect(mocked.logout).toHaveBeenCalledTimes(2);
  });
});
