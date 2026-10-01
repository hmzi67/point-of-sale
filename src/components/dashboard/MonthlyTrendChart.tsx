import { useId } from "react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { ChartNoAxesCombined } from "lucide-react";
import { useChartColors } from "../../hooks/useChartColors";
import { formatCompactTick, formatMinor, formatShortDate, minorToDecimal } from "../../utils/format";
import { ChartTooltipCard } from "../ui/ChartTooltipCard";
import type { DailySales } from "../../types";

interface MonthlyTrendChartProps {
  series: DailySales[];
  currency: string;
  isLoading: boolean;
}

function ChartTooltip({ active, payload, currency }: {
  active?: boolean;
  payload?: { payload: DailySales }[];
  currency: string;
}) {
  if (!active || !payload?.length) return null;
  const point = payload[0].payload;
  return (
    <ChartTooltipCard>
      <p className="text-xs text-fg-muted">{formatShortDate(point.date)}</p>
      <p className="mt-1 font-semibold text-accent-text">{formatMinor(point.totalMinor, currency)}</p>
    </ChartTooltipCard>
  );
}

export function MonthlyTrendChart({ series, currency, isLoading }: MonthlyTrendChartProps) {
  const chartColors = useChartColors();
  const gradientId = useId().replace(/:/g, "");
  const chartData = series.map((point) => ({ ...point, totalDecimal: minorToDecimal(point.totalMinor) }));
  const total = series.reduce((sum, point) => sum + point.totalMinor, 0);
  const hasAnySales = series.some((point) => point.totalMinor > 0);
  const bestDay = hasAnySales ? series.reduce((best, point) => point.totalMinor > best.totalMinor ? point : best) : null;
  const monthLabel = new Intl.DateTimeFormat(undefined, { month: "long", year: "numeric" }).format(new Date());

  return (
    <div className="min-w-0 overflow-hidden rounded-2xl border border-line bg-surface">
      <div className="flex flex-wrap items-start justify-between gap-4 px-5 pt-6 sm:px-6">
        <div>
          <h3 className="text-sm font-semibold text-fg">Sales overview</h3>
          <p className="mt-1 text-xs text-fg-muted">Your sales performance this month</p>
        </div>
        <span className="rounded-lg border border-line bg-surface-2 px-3 py-1.5 text-xs font-medium text-fg-2">{monthLabel}</span>
      </div>
      <div className="flex flex-wrap items-end justify-between gap-3 px-5 pt-6 sm:px-6">
        <div>
          <p className="text-xs text-fg-muted">Month-to-date sales</p>
          <p className="mt-2 break-words text-3xl font-bold tracking-tight text-fg tabular-nums">
            {isLoading ? <span className="inline-block h-9 w-40 animate-pulse rounded bg-surface-3" /> : formatMinor(total, currency)}
          </p>
        </div>
        <span className="mb-1 flex items-center gap-2 text-xs text-fg-muted"><span className="h-2 w-2 rounded-full bg-accent" />Daily sales</span>
      </div>

      {isLoading ? (
        <div className="mx-5 my-6 h-72 animate-pulse rounded-xl bg-surface-2" />
      ) : !hasAnySales ? (
        <div className="flex h-84 flex-col items-center justify-center gap-3 px-5 text-center">
          <span className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent-text"><ChartNoAxesCombined className="h-6 w-6" /></span>
          <p className="text-sm font-medium text-fg">Your next sale starts the story</p>
          <p className="max-w-xs text-xs leading-relaxed text-fg-muted">No sales recorded this month. Completed sales will appear here automatically.</p>
        </div>
      ) : (
        <div className="mt-6 h-78 pr-4 pb-3 sm:pr-6">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart accessibilityLayer data={chartData} margin={{ top: 12, right: 8, left: 4, bottom: 4 }}>
              <defs>
                <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor={chartColors.series} stopOpacity={0.28} />
                  <stop offset="95%" stopColor={chartColors.series} stopOpacity={0.01} />
                </linearGradient>
              </defs>
              <CartesianGrid vertical={false} stroke={chartColors.grid} strokeDasharray="4 5" />
              <XAxis dataKey="date" tickFormatter={formatShortDate}
                tick={{ fontSize: 11, fill: chartColors.tick }} axisLine={false} tickLine={false}
                tickMargin={12} interval="preserveStartEnd" minTickGap={32} />
              <YAxis tick={{ fontSize: 11, fill: chartColors.tick }} axisLine={false} tickLine={false}
                width={56} tickMargin={8}
                tickFormatter={formatCompactTick} />
              <Tooltip content={<ChartTooltip currency={currency} />} cursor={{ stroke: chartColors.series, strokeDasharray: "4 4", strokeOpacity: 0.4 }} />
              <Area type="monotone" dataKey="totalDecimal" stroke={chartColors.series} strokeWidth={2.5}
                fill={`url(#${gradientId})`} dot={chartData.length === 1 ? { r: 4 } : false}
                activeDot={{ r: 5, strokeWidth: 3, stroke: chartColors.cursor }} isAnimationActive={false} />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
      <div className="grid grid-cols-1 divide-y divide-line border-t border-line bg-surface-2/30 sm:grid-cols-2 sm:divide-x sm:divide-y-0">
        <div className="px-6 py-5">
          <p className="text-xs text-fg-muted">Best sales day</p>
          <p className="mt-2 text-sm font-semibold text-fg">{isLoading ? "—" : bestDay ? formatShortDate(bestDay.date) : "No sales yet"}</p>
        </div>
        <div className="px-6 py-5">
          <p className="text-xs text-fg-muted">Highest daily sales</p>
          <p className="mt-2 text-sm font-semibold text-accent-text tabular-nums">{isLoading ? "—" : formatMinor(bestDay?.totalMinor ?? 0, currency)}</p>
        </div>
      </div>
    </div>
  );
}

