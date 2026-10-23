import { beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "../services/reportsService";
import type { SalesSummary } from "../types";
import { useReportsStore } from "./reportsStore";

vi.mock("../services/reportsService", () => ({
  getSalesOverTime: vi.fn(),
  getSalesSummary: vi.fn(),
  getTopItems: vi.fn(),
}));

const mocked = vi.mocked(service);
const store = () => useReportsStore.getState();

beforeEach(() => {
  vi.resetAllMocks();
  mocked.getSalesSummary.mockResolvedValue({ totalSalesMinor: 100 } as SalesSummary);
  mocked.getTopItems.mockResolvedValue([]);
  mocked.getSalesOverTime.mockResolvedValue([]);
});

describe("reportsStore", () => {
  it("reloads every panel for a custom range with the current sort", async () => {
    useReportsStore.setState({ topItemsSort: "revenue" });
    store().setCustomRange("2026-01-01", "2026-01-31");
    await vi.waitFor(() => expect(store().isLoading).toBe(false));
    expect(store()).toMatchObject({ preset: "custom", startDate: "2026-01-01", endDate: "2026-01-31" });
    expect(mocked.getTopItems).toHaveBeenCalledWith("2026-01-01", "2026-01-31", 10, "revenue");
    expect(store().summary).toEqual({ totalSalesMinor: 100 });
  });

  it("reports a failed load without dropping the previous summary", async () => {
    useReportsStore.setState({ summary: { totalSalesMinor: 5 } as SalesSummary });
    mocked.getSalesSummary.mockRejectedValue(new Error("range too large"));
    await store().load();
    expect(store()).toMatchObject({ error: "range too large", isLoading: false, summary: { totalSalesMinor: 5 } });
  });
});
