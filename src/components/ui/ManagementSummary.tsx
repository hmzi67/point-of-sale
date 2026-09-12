import type { LucideIcon } from "lucide-react";

interface SummaryItem {
  label: string;
  value: string | number;
  detail: string;
  icon: LucideIcon;
  tone?: "accent" | "success" | "warning";
}

export function ManagementSummary({ items, isLoading }: { items: SummaryItem[]; isLoading: boolean }) {
  const tones = { accent: "bg-accent-soft text-accent-text", success: "bg-success-soft text-success-text", warning: "bg-warning-soft text-warning-text" };
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
      {items.map(({ label, value, detail, icon: Icon, tone = "accent" }, index) => (
        <div key={label} className={`rounded-2xl border p-5 ${index === 0 ? "border-accent/25 bg-linear-to-br from-accent-soft to-surface" : "border-line bg-surface"}`}>
          <div className="flex items-center justify-between gap-3">
            <p className="text-xs font-medium text-fg-muted">{label}</p>
            <span className={`flex h-10 w-10 shrink-0 items-center justify-center rounded-xl ${tones[tone]}`}><Icon className="h-5 w-5" strokeWidth={1.75} /></span>
          </div>
          <p className="mt-3 break-words text-2xl font-bold tracking-tight text-fg tabular-nums">{isLoading ? <span className="inline-block h-8 w-24 animate-pulse rounded bg-surface-3" /> : value}</p>
          <p className="mt-3 text-[11px] text-fg-muted">{detail}</p>
        </div>
      ))}
    </div>
  );
}

export function PersonAvatar({ name }: { name: string }) {
  return <span aria-hidden="true" className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-accent/10 bg-accent-soft text-xs font-bold text-accent-text">{name.trim().split(/\s+/).slice(0, 2).map((part) => part[0]).join("").toUpperCase()}</span>;
}

export function ActiveBadge({ active }: { active: boolean }) {
  return <span className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-lg px-2.5 py-1 text-[11px] font-medium ${active ? "bg-success-soft text-success-text" : "bg-surface-3 text-fg-muted"}`}><span className="h-1.5 w-1.5 rounded-full bg-current" />{active ? "Active" : "Deactivated"}</span>;
}
