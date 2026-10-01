import { useId } from "react";
import { ChartNoAxesCombined } from "lucide-react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useChartColors } from "../../hooks/useChartColors";
import { formatCompactTick, formatMinor, formatShortDate, minorToDecimal } from "../../utils/format";
import { ChartTooltipCard } from "../ui/ChartTooltipCard";
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
    <ChartTooltipCard>
      <p className="font-medium text-fg">{formatShortDate(point.date)}</p>
      <p className="text-fg-2">{formatMinor(point.totalMinor, currency)}</p>
      <p className="text-xs text-fg-muted">
        {point.transactionCount} sale{point.transactionCount === 1 ? "" : "s"}
      </p>
    </ChartTooltipCard>
  );
}

/**
 * Single-series magnitude-over-time — a fixed brand hue, no legend needed
 * (the section title names the series), thin bars with rounded data-ends,
 * a recessive grid, and a hover tooltip per the dataviz house style.
 */
export function SalesChart({ series, currency, isLoading }: SalesChartProps) {
  const chartColors = useChartColors();
  const gradientId = useId().replace(/:/g, "");

  const chartData = series.map((point) => ({ ...point, totalDecimal: minorToDecimal(point.totalMinor) }));
  const hasAnySales = series.some((point) => point.totalMinor > 0);

  return (
    <div className="min-w-0 rounded-2xl border border-line bg-surface p-5 sm:p-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h3 className="text-sm font-semibold text-fg">Sales performance</h3>
          <p className="mt-1.5 text-xs text-fg-muted">Daily sales across your selected period</p>
        </div>
        <span className="flex items-center gap-2 rounded-lg border border-line px-3 py-1.5 text-xs text-fg-muted"><span className="h-2 w-2 rounded-full bg-accent" />Sales · {currency}</span>
      </div>

      {isLoading ? (
        <div className="mt-6 h-72 animate-pulse rounded bg-surface-2" />
      ) : !hasAnySales ? (
        <div className="mt-6 flex h-72 flex-col items-center justify-center gap-3 text-sm text-fg-muted">
          <span className="rounded-2xl bg-accent-soft p-4 text-accent-text"><ChartNoAxesCombined className="h-7 w-7" /></span>
          <p className="font-medium text-fg">No sales in this period</p>
          <p className="text-xs">Choose another date range to explore your sales.</p>
        </div>
      ) : (
        <div className="mt-6 h-72">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart accessibilityLayer data={chartData} margin={{ top: 8, right: 8, left: 0, bottom: 0 }} barCategoryGap="20%">
              <defs><linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor={chartColors.series} /><stop offset="100%" stopColor={chartColors.series} stopOpacity={0.35} /></linearGradient></defs>
              <CartesianGrid vertical={false} stroke={chartColors.grid} strokeDasharray="4 5" />
              <XAxis
                dataKey="date"
                tickFormatter={formatShortDate}
                tick={{ fontSize: 11, fill: chartColors.tick }}
                axisLine={false}
                tickMargin={10}
                tickLine={false}
                interval="preserveStartEnd"
                minTickGap={24}
              />
              <YAxis
                tick={{ fontSize: 11, fill: chartColors.tick }}
                axisLine={false}
                tickLine={false}
                width={48}
                tickFormatter={formatCompactTick}
              />
              <Tooltip content={<ChartTooltip currency={currency} />} cursor={{ fill: chartColors.cursor }} />
              <Bar dataKey="totalDecimal" fill={`url(#${gradientId})`} radius={[7, 7, 0, 0]} maxBarSize={56} isAnimationActive={false} />
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
