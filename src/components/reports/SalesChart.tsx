import { useEffect, useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { formatMinor, formatShortDate, minorToDecimal } from "../../utils/format";
import type { DailySales } from "../../types";

interface SalesChartProps {
  series: DailySales[];
  currency: string;
  isLoading: boolean;
}

interface TooltipPayloadItem {
  payload: DailySales;
}

function ChartTooltip({ active, payload, currency }: { active?: boolean; payload?: TooltipPayloadItem[]; currency: string }) {
  if (!active || !payload?.length) return null;
  const point = payload[0].payload;
  return (
    <div className="rounded-md border border-slate-200 bg-white px-3 py-2 text-sm shadow-lg">
      <p className="font-medium text-slate-900">{formatShortDate(point.date)}</p>
      <p className="text-slate-600">{formatMinor(point.totalMinor, currency)}</p>
      <p className="text-xs text-slate-400">
        {point.transactionCount} sale{point.transactionCount === 1 ? "" : "s"}
      </p>
    </div>
  );
}

/**
 * Single-series magnitude-over-time — a fixed brand hue, no legend needed
 * (the section title names the series), thin bars with rounded data-ends,
 * a recessive grid, and a hover tooltip per the dataviz house style.
 */
export function SalesChart({ series, currency, isLoading }: SalesChartProps) {
  // Read the real brand-600 token at runtime rather than duplicating its
  // oklch value as a hardcoded hex — this always matches the rest of the UI.
  const [barColor, setBarColor] = useState("#4338ca");
  useEffect(() => {
    const value = getComputedStyle(document.documentElement).getPropertyValue("--color-brand-600").trim();
    if (value) setBarColor(value);
  }, []);

  const chartData = series.map((point) => ({ ...point, totalDecimal: minorToDecimal(point.totalMinor) }));
  const hasAnySales = series.some((point) => point.totalMinor > 0);

  return (
    <div className="rounded-2xl border border-slate-200 bg-white p-4 shadow-soft">
      <h3 className="text-sm font-semibold text-slate-900">Sales by day</h3>

      {isLoading ? (
        <div className="mt-3 h-64 animate-pulse rounded bg-slate-50" />
      ) : !hasAnySales ? (
        <div className="mt-3 flex h-64 items-center justify-center text-sm text-slate-400">
          No sales in this range
        </div>
      ) : (
        <div className="mt-3 h-64">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }} barCategoryGap="20%">
              <CartesianGrid vertical={false} stroke="#e2e8f0" strokeDasharray="3 3" />
              <XAxis
                dataKey="date"
                tickFormatter={formatShortDate}
                tick={{ fontSize: 11, fill: "#64748b" }}
                axisLine={{ stroke: "#e2e8f0" }}
                tickLine={false}
                interval="preserveStartEnd"
                minTickGap={24}
              />
              <YAxis
                tick={{ fontSize: 11, fill: "#64748b" }}
                axisLine={false}
                tickLine={false}
                width={48}
                tickFormatter={(value: number) => (value >= 1000 ? `${Math.round(value / 1000)}k` : String(value))}
              />
              <Tooltip content={<ChartTooltip currency={currency} />} cursor={{ fill: "#f1f5f9" }} />
              <Bar dataKey="totalDecimal" fill={barColor} radius={[4, 4, 0, 0]} maxBarSize={40} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

// Recharts is a large dependency (~150KB) with no reason to load on any
// screen but this one — ReportsPage pulls this component in via `React.lazy`,
// which requires a default export.
export default SalesChart;
