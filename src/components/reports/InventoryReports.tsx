import { useState } from "react";
import { useReport } from "../../hooks/useReport";
import { getExpiryReport, getInventoryUsage, getStockLevels } from "../../services/insightsService";
import { formatMinor, formatQty, minorToDecimal } from "../../utils/format";
import { DataTable, EmptyRow } from "../ui/DataTable";
import { CsvButton, ReportState, SegmentedControl, StatCard, StatGrid } from "./ReportBits";

type Sub = "levels" | "usage" | "expiry";

/** Stock level & valuation, usage & turnover, and product expiry. */
export function InventoryReports({ startDate, endDate, currency }: { startDate: string; endDate: string; currency: string }) {
  const [sub, setSub] = useState<Sub>("levels");
  return (
    <div className="space-y-4">
      <SegmentedControl
        value={sub}
        onChange={setSub}
        options={[
          { value: "levels", label: "Stock level & valuation" },
          { value: "usage", label: "Usage & turnover" },
          { value: "expiry", label: "Expiry" },
        ]}
      />
      {sub === "levels" && <StockLevels currency={currency} />}
      {sub === "usage" && <Usage startDate={startDate} endDate={endDate} currency={currency} />}
      {sub === "expiry" && <Expiry />}
    </div>
  );
}

function StockLevels({ currency }: { currency: string }) {
  const [onlyLow, setOnlyLow] = useState(false);
  const { data, isLoading, error } = useReport(getStockLevels, []);
  const money = (minor: number) => formatMinor(minor, currency);
  const rows = (data?.rows ?? []).filter((r) => !onlyLow || r.isLowStock);

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <StatGrid>
            <StatCard label="Stock value (cost)" value={money(data.costValueMinor)} />
            <StatCard label="Stock value (retail)" value={money(data.retailValueMinor)} detail={`${money(data.retailValueMinor - data.costValueMinor)} potential margin`} />
            <StatCard label="Low stock" value={String(data.lowStockCount)} detail="At or under their reorder level" tone="warning" />
            <StatCard label="Out of stock" value={String(data.outOfStockCount)} tone="danger" />
          </StatGrid>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <label className="flex items-center gap-2 text-sm text-fg-2">
              <input type="checkbox" checked={onlyLow} onChange={(e) => setOnlyLow(e.target.checked)} /> Low-stock report only
            </label>
            <CsvButton
              filename="stock-levels.csv"
              rows={() => [
                ["Item", "Category", "In stock", "Reorder level", "On order", "Unit cost", "Price", "Value at cost", "Value at retail"],
                ...rows.map((r) => [r.itemName, r.categoryName ?? "", r.stockQty, r.lowStockThreshold, r.incomingQty, minorToDecimal(r.costMinor), minorToDecimal(r.priceMinor), minorToDecimal(r.costValueMinor), minorToDecimal(r.retailValueMinor)]),
              ]}
            />
          </div>
          <DataTable head={["Item", "In stock", "Reorder at", "On order", "Unit cost", "Value (cost)", "Value (retail)"]}>
            {rows.map((r) => (
              <tr key={r.itemId} className="tabular-nums">
                <td className="px-4 py-2.5">
                  <span className="font-medium text-fg">{r.itemName}</span>
                  <span className="block text-xs text-fg-muted">{r.categoryName ?? "Uncategorised"}</span>
                </td>
                <td className={`px-4 py-2.5 font-semibold ${r.stockQty <= 0 ? "text-danger-text" : r.isLowStock ? "text-warning-text" : "text-fg"}`}>{formatQty(r.stockQty, r.unit)}</td>
                <td className="px-4 py-2.5 text-fg-muted">{r.lowStockThreshold}</td>
                <td className="px-4 py-2.5 text-fg-2">{r.incomingQty > 0 ? formatQty(r.incomingQty, r.unit) : "—"}</td>
                <td className="px-4 py-2.5 text-right">{money(r.costMinor)}</td>
                <td className="px-4 py-2.5 text-right">{money(r.costValueMinor)}</td>
                <td className="px-4 py-2.5 text-right">{money(r.retailValueMinor)}</td>
              </tr>
            ))}
            {rows.length === 0 && <EmptyRow span={7} text={onlyLow ? "Nothing is low on stock." : "No active items."} />}
          </DataTable>
        </div>
      )}
    </ReportState>
  );
}

function Usage({ startDate, endDate, currency }: { startDate: string; endDate: string; currency: string }) {
  const { data, isLoading, error } = useReport(() => getInventoryUsage(startDate, endDate), [startDate, endDate]);
  const rows = data ?? [];
  const cogs = rows.reduce((sum, r) => sum + r.cogsMinor, 0);
  const moving = rows.filter((r) => r.soldQty > 0).length;

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <StatGrid>
            <StatCard label="Cost of goods used" value={formatMinor(cogs, currency)} />
            <StatCard label="Items that sold" value={`${moving} / ${rows.length}`} />
            <StatCard label="Received" value={formatQty(rows.reduce((s, r) => s + r.receivedQty, 0))} detail="Units in via goods received" />
            <StatCard label="Damaged / lost" value={formatQty(rows.reduce((s, r) => s + r.damagedQty, 0))} tone="danger" />
          </StatGrid>
          <div className="flex justify-end">
            <CsvButton
              filename={`inventory-usage-${startDate}-to-${endDate}.csv`}
              rows={() => [
                ["Item", "Opening", "Received", "Sold", "Returned", "Damaged", "Adjusted", "Closing", "Turnover", "Days of cover", "COGS"],
                ...rows.map((r) => [r.itemName, r.openingQty, r.receivedQty, r.soldQty, r.returnedQty, r.damagedQty, r.adjustedQty, r.closingQty, r.turnover, r.daysOfCover ?? "", minorToDecimal(r.cogsMinor)]),
              ]}
            />
          </div>
          <DataTable head={["Item", "Opening", "In", "Sold", "Returned", "Damaged", "Adjusted", "Closing", "Turnover", "Cover"]}>
            {rows.map((r) => (
              <tr key={r.itemId} className="tabular-nums">
                <td className="px-4 py-2.5 font-medium text-fg">{r.itemName}</td>
                <td className="px-4 py-2.5 text-fg-muted">{formatQty(r.openingQty, r.unit)}</td>
                <td className="px-4 py-2.5 text-success-text">{r.receivedQty ? `+${formatQty(r.receivedQty, r.unit)}` : ""}</td>
                <td className="px-4 py-2.5 font-semibold">{formatQty(r.soldQty, r.unit)}</td>
                <td className="px-4 py-2.5 text-fg-2">{r.returnedQty ? formatQty(r.returnedQty, r.unit) : ""}</td>
                <td className="px-4 py-2.5 text-danger-text">{r.damagedQty ? formatQty(r.damagedQty, r.unit) : ""}</td>
                <td className="px-4 py-2.5 text-fg-2">{r.adjustedQty ? formatQty(r.adjustedQty, r.unit) : ""}</td>
                <td className="px-4 py-2.5">{formatQty(r.closingQty, r.unit)}</td>
                <td className="px-4 py-2.5">{r.turnover ? `${r.turnover}×` : "—"}</td>
                <td className="px-4 py-2.5 text-xs text-fg-muted">{r.daysOfCover !== null ? `${r.daysOfCover} days` : "—"}</td>
              </tr>
            ))}
            {rows.length === 0 && <EmptyRow span={10} text="No active items." />}
          </DataTable>
        </div>
      )}
    </ReportState>
  );
}

function Expiry() {
  const [withinDays, setWithinDays] = useState(30);
  const { data, isLoading, error } = useReport(() => getExpiryReport(withinDays), [withinDays]);
  const rows = data ?? [];
  const expired = rows.filter((r) => r.daysLeft < 0).length;

  return (
    <ReportState isLoading={isLoading && !data} error={error}>
      {data && (
        <div className="space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <SegmentedControl
              value={String(withinDays)}
              onChange={(v) => setWithinDays(Number(v))}
              options={[
                { value: "7", label: "7 days" },
                { value: "30", label: "30 days" },
                { value: "90", label: "90 days" },
              ]}
            />
            <span className="text-xs text-fg-muted">
              {expired} batch(es) already expired · expiry dates come from goods received notes
            </span>
          </div>
          <DataTable head={["Item", "Batch", "Expires", "Received", "In stock", "Supplier"]}>
            {rows.map((r, i) => (
              <tr key={`${r.grnId}-${i}`} className="tabular-nums">
                <td className="px-4 py-2.5 font-medium text-fg">{r.itemName}</td>
                <td className="px-4 py-2.5 text-fg-2">{r.batchNo ?? "—"}</td>
                <td className={`px-4 py-2.5 font-semibold ${r.daysLeft < 0 ? "text-danger-text" : r.daysLeft <= 7 ? "text-warning-text" : "text-fg"}`}>
                  {r.expiryDate}
                  <span className="block text-xs font-normal">{r.daysLeft < 0 ? `expired ${-r.daysLeft}d ago` : r.daysLeft === 0 ? "today" : `in ${r.daysLeft}d`}</span>
                </td>
                <td className="px-4 py-2.5 text-xs text-fg-muted">{formatQty(r.qtyReceived)} on {r.receivedDate} (GRN #{r.grnId})</td>
                <td className="px-4 py-2.5">{formatQty(r.stockQty)}</td>
                <td className="px-4 py-2.5 text-fg-2">{r.supplierName}</td>
              </tr>
            ))}
            {rows.length === 0 && <EmptyRow span={6} text="Nothing in stock expires in this window." />}
          </DataTable>
        </div>
      )}
    </ReportState>
  );
}
