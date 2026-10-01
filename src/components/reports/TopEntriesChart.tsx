import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useChartColors } from "../../hooks/useChartColors";
import { formatCompactTick, formatMinor, minorToDecimal } from "../../utils/format";
import { ChartTooltipCard } from "../ui/ChartTooltipCard";

export interface TopEntry {
  label: string;
  valueMinor: number;
}

interface TopEntriesChartProps {
  title: string;
  entries: TopEntry[];
  currency: string;
  isLoading: boolean;
  emptyLabel?: string;
}

interface TooltipPayloadItem {
  payload: TopEntry;
}

function ChartTooltip({
  active,
  payload,
  currency,
}: {
  active?: boolean;
  payload?: TooltipPayloadItem[];
  currency: string;
}) {
  if (!active || !payload?.length) return null;
  const point = payload[0].payload;
  return (
    <ChartTooltipCard>
      <p className="font-medium text-fg">{point.label}</p>
      <p className="text-fg-2">{formatMinor(point.valueMinor, currency)}</p>
    </ChartTooltipCard>
  );
}

/**
 * Horizontal ranked-bar chart — top entries by revenue, the same house
 * style (fixed brand hue, recessive grid, hover tooltip) as `SalesChart`'s
 * time series, just ranked-category instead of time on the axis. Shared by
 * the Product Wise and Table Wise Sales views so each gets a visual
 * alongside its table, not just a plain list of numbers.
 */
export function TopEntriesChart({
  title,
  entries,
  currency,
  isLoading,
  emptyLabel = "No sales in this range",
}: TopEntriesChartProps) {
  const chartColors = useChartColors();

  // Highest first, top 8 — a chart with dozens of bars reads worse than
  // the table below it, which still lists every row.
  const top = [...entries].sort((a, b) => b.valueMinor - a.valueMinor).slice(0, 8);
  const chartData = top.map((entry) => ({ ...entry, valueDecimal: minorToDecimal(entry.valueMinor) }));
  const hasAnySales = entries.some((entry) => entry.valueMinor > 0);

  return (
    <div className="rounded-2xl border border-line bg-surface p-5 sm:p-6">
      <h3 className="text-sm font-semibold text-fg">{title}</h3>

      {isLoading ? (
        <div className="mt-6 h-64 animate-pulse rounded bg-surface-2" />
      ) : !hasAnySales ? (
        <div className="mt-3 flex h-64 items-center justify-center text-sm text-fg-muted">{emptyLabel}</div>
      ) : (
        <div className="mt-6 h-64">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart accessibilityLayer data={chartData} layout="vertical" margin={{ top: 8, right: 16, left: 8, bottom: 0 }} barCategoryGap="24%">
              <CartesianGrid horizontal={false} stroke={chartColors.grid} strokeDasharray="3 3" />
              <XAxis
                type="number"
                tick={{ fontSize: 11, fill: chartColors.tick }}
                axisLine={{ stroke: chartColors.grid }}
                tickLine={false}
                tickFormatter={formatCompactTick}
              />
              <YAxis
                type="category"
                dataKey="label"
                tick={{ fontSize: 11, fill: chartColors.tick }}
                axisLine={false}
                tickLine={false}
                width={96}
                interval={0}
              />
              <Tooltip content={<ChartTooltip currency={currency} />} cursor={{ fill: chartColors.cursor }} />
              <Bar dataKey="valueDecimal" fill={chartColors.series} radius={[0, 4, 4, 0]} maxBarSize={22} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

// Recharts is a large dependency (~150KB) with no reason to load on any other
// screen — ReportsPage pulls this in via `React.lazy`, mapping the named
// export to the `default` that `lazy` expects.
