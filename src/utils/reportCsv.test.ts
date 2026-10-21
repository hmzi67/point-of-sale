import { describe, expect, it } from "vitest";
import type { AppConfig, DailySales, SalesSummary, TopItem } from "../types";
import { buildReportCsv } from "./reportCsv";

const summary = {
  startDate: "2026-10-01",
  endDate: "2026-10-07",
  totalSalesMinor: 1234567,
  refundsMinor: 0,
  netSalesMinor: 1234567,
  transactionCount: 3,
  averageSaleMinor: 411522,
} as SalesSummary;

describe("buildReportCsv", () => {
  const csv = buildReportCsv({
    summary,
    topItems: [{ itemId: 1, itemName: "Rice, basmati", qtySold: 12.499999999999998, revenueMinor: 99950 } as TopItem],
    series: [{ date: "2026-10-01", totalMinor: 5, refundsMinor: 0, netMinor: 5, transactionCount: 1 } as DailySales],
    config: {} as AppConfig,
  });
  const sections = csv.split("\r\n\r\n");

  it("writes money as 2-decimal major units", () => {
    expect(sections[0]).toContain("Total sales,12345.67");
    expect(sections[0]).toContain("Average sale,4115.22");
    expect(sections[2]).toContain("2026-10-01,0.05,1");
  });

  it("rounds float noise out of quantities and quotes names with commas", () => {
    expect(sections[1]).toContain('"Rice, basmati",12.5,999.50');
  });

  it("keeps three blank-line-separated sections", () => {
    expect(sections).toHaveLength(3);
    expect(sections[1].split("\r\n")[0]).toBe("Top-selling items");
  });
});
