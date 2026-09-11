import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useChartColors } from "../../hooks/useChartColors";
import { formatMinor, formatShortDate, minorToDecimal } from "../../utils/format";
import type { DailySales } from "../../types";

interface MonthlyTrendChartProps {
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
    </div>
  );
}

/** This month's sales, day by day — the trend line that makes "is business
 * up or down this month" readable at a glance from the landing page. */
export function MonthlyTrendChart({ series, currency, isLoading }: MonthlyTrendChartProps) {
  const chartColors = useChartColors();

  const chartData = series.map((point) => ({ ...point, totalDecimal: minorToDecimal(point.totalMinor) }));
  const hasAnySales = series.some((point) => point.totalMinor > 0);

  return (
    <div className="rounded-lg border border-line bg-surface p-4">
      <h3 className="text-sm font-semibold text-fg">Sales this month</h3>

      {isLoading ? (
        <div className="mt-3 h-64 animate-pulse rounded bg-surface-2" />
      ) : !hasAnySales ? (
        <div className="mt-3 flex h-64 items-center justify-center text-sm text-fg-muted">
          No sales so far this month
        </div>
      ) : (
        <div className="mt-3 h-64">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }}>
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
              <Tooltip content={<ChartTooltip currency={currency} />} cursor={{ stroke: chartColors.grid }} />
              <Line
                type="monotone"
                dataKey="totalDecimal"
                stroke={chartColors.series}
                strokeWidth={2}
                dot={false}
                activeDot={{ r: 4 }}
              />
            </LineChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

// Recharts is a large dependency with no reason to load before the dashboard
// actually renders its chart — DashboardPage pulls this in via `React.lazy`,
// which requires a default export.
export default MonthlyTrendChart;
