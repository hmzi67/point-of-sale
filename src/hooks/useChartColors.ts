import { useEffect, useState } from "react";
import { useUiPrefsStore } from "../store";

/** The handful of theme colours recharts needs as literal values. */
export interface ChartColors {
  /** Series fill/stroke — the brand accent. */
  series: string;
  /** Gridlines and axis lines. */
  grid: string;
  /** Tick labels. */
  tick: string;
  /** Hover cursor (bar highlight band / line crosshair). */
  cursor: string;
}

/* Sensible dark-theme literals for the first render, before the effect
 * has read the real tokens — matches `styles.css`'s dark block so a chart
 * never flashes an off-theme colour. */
const INITIAL: ChartColors = { series: "#00e5a0", grid: "#1e2a25", tick: "#93a39b", cursor: "#18221e" };

/**
 * recharts takes colours as props, not classes, so it can't resolve
 * `bg-accent` the way the rest of the UI does. This reads the *same* `--t-*`
 * variables the tokens are built from (see `styles.css`) off the root
 * element, and re-reads them whenever the theme flips, so a chart that is
 * on screen during a toggle repaints with the new theme like everything
 * else — a one-off read on mount would leave it stranded in the old one.
 */
export function useChartColors(): ChartColors {
  const theme = useUiPrefsStore((state) => state.theme);
  const [colors, setColors] = useState<ChartColors>(INITIAL);

  useEffect(() => {
    const style = getComputedStyle(document.documentElement);
    const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
    setColors({
      series: read("--t-accent", INITIAL.series),
      grid: read("--t-line", INITIAL.grid),
      tick: read("--t-fg-muted", INITIAL.tick),
      cursor: read("--t-surface-2", INITIAL.cursor),
    });
    // `theme` is the dependency on purpose — it's what triggers the re-read.
  }, [theme]);

  return colors;
}
