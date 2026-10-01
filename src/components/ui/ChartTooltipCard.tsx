import type { ReactNode } from "react";

/** The floating card every recharts `<Tooltip content>` renders into, so
 * the charts' hover cards look the same; each chart supplies its own lines. */
export function ChartTooltipCard({ children }: { children: ReactNode }) {
  return <div className="rounded-xl border border-line-strong bg-surface px-4 py-3 text-sm shadow-lg">{children}</div>;
}
