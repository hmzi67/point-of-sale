import { beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "../services/shiftsService";
import type { Shift, ShiftSummary } from "../types";
import { useShiftStore } from "./shiftStore";

vi.mock("../services/shiftsService", () => ({ closeShift: vi.fn(), getOpenShift: vi.fn(), openShift: vi.fn() }));

const mocked = vi.mocked(service);
const store = () => useShiftStore.getState();
const shift = { id: 4, openingBalanceMinor: 5000 } as Shift;

beforeEach(() => {
  vi.resetAllMocks();
  useShiftStore.setState({ openShift: null, isLoaded: false, isLoading: false, error: null });
});

describe("shiftStore", () => {
  it("loads the open shift, and still counts as loaded when the read fails", async () => {
    mocked.getOpenShift.mockResolvedValue(shift);
    await store().load();
    expect(store()).toMatchObject({ openShift: shift, isLoaded: true, isLoading: false });

    mocked.getOpenShift.mockRejectedValue(new Error("db locked"));
    await store().load();
    expect(store()).toMatchObject({ error: "db locked", isLoaded: true, isLoading: false });
  });

  it("closes the open shift by id and clears it", async () => {
    useShiftStore.setState({ openShift: shift });
    mocked.closeShift.mockResolvedValue({ expectedCashMinor: 1 } as ShiftSummary);
    await store().close(7000);
    expect(mocked.closeShift).toHaveBeenCalledWith(4, 7000);
    expect(store().openShift).toBeNull();
  });

  it("refuses to close with no open shift, and keeps it open when closing fails", async () => {
    await expect(store().close(1)).rejects.toThrow("No open shift");
    useShiftStore.setState({ openShift: shift });
    mocked.closeShift.mockRejectedValue(new Error("already closed"));
    await expect(store().close(1)).rejects.toThrow("already closed");
    expect(store()).toMatchObject({ openShift: shift, error: "already closed" });
  });
});
