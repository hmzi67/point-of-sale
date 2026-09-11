import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useChartColors } from "../../hooks/useChartColors";
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
    <div className="rounded-md border border-line bg-surface px-3 py-2 text-sm shadow-lg">
      <p className="font-medium text-fg">{formatShortDate(point.date)}</p>
      <p className="text-fg-2">{formatMinor(point.totalMinor, currency)}</p>
      <p className="text-xs text-fg-muted">
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
  const chartColors = useChartColors();

  const chartData = series.map((point) => ({ ...point, totalDecimal: minorToDecimal(point.totalMinor) }));
  const hasAnySales = series.some((point) => point.totalMinor > 0);

  return (
    <div className="rounded-2xl border border-line bg-surface p-4 shadow-soft">
      <h3 className="text-sm font-semibold text-fg">Sales by day</h3>

      {isLoading ? (
        <div className="mt-3 h-64 animate-pulse rounded bg-surface-2" />
      ) : !hasAnySales ? (
        <div className="mt-3 flex h-64 items-center justify-center text-sm text-fg-muted">
          No sales in this range
        </div>
      ) : (
        <div className="mt-3 h-64">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }} barCategoryGap="20%">
              <CartesianGrid vertical={false} stroke={chartColors.grid} strokeDasharray="3 3" />
              <XAxis
                dataKey="date"
                tickFormatter={formatShortDate}
                tick={{ fontSize: 11, fill: chartColors.tick }}
                axisLine={{ stroke: chartColors.grid }}
                tickLine={false}
                interval="preserveStartEnd"
                minTickGap={24}
              />
              <YAxis
                tick={{ fontSize: 11, fill: chartColors.tick }}
                axisLine={false}
                tickLine={false}
                width={48}
                tickFormatter={(value: number) => (value >= 1000 ? `${Math.round(value / 1000)}k` : String(value))}
              />
              <Tooltip content={<ChartTooltip currency={currency} />} cursor={{ fill: chartColors.cursor }} />
              <Bar dataKey="totalDecimal" fill={chartColors.series} radius={[4, 4, 0, 0]} maxBarSize={40} />
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
