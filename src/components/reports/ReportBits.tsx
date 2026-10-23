import type { ReactNode } from "react";
import { Download } from "lucide-react";
import { buildCsv, downloadTextFile } from "../../utils/csv";

/** One headline figure on a report tab. */
export function StatCard({ label, value, detail, tone = "fg" }: { label: string; value: string; detail?: string; tone?: "fg" | "success" | "danger" | "warning" }) {
  const color = { fg: "text-fg", success: "text-success-text", danger: "text-danger-text", warning: "text-warning-text" }[tone];
  return (
    <div className="rounded-2xl border border-line bg-surface p-4">
      <p className="text-xs font-medium text-fg-muted">{label}</p>
      <p className={`mt-2 text-xl font-bold tabular-nums ${color}`}>{value}</p>
      {detail && <p className="mt-1 text-[11px] text-fg-muted">{detail}</p>}
    </div>
  );
}

export function StatGrid({ children }: { children: ReactNode }) {
  return <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">{children}</div>;
}

/** Saves `rows` (first row = header) as a CSV file. */
export function CsvButton({ rows, filename }: { rows: () => Array<Array<string | number>>; filename: string }) {
  return (
    <button
      type="button"
      onClick={() => void downloadTextFile(buildCsv(rows()), filename)}
      className="flex items-center gap-1.5 rounded-xl border border-line px-3 py-2 text-xs font-medium text-fg-2 hover:bg-surface-2"
    >
      <Download className="h-3.5 w-3.5" /> Export CSV
    </button>
  );
}

export function ReportState({ isLoading, error, children }: { isLoading: boolean; error: string | null; children: ReactNode }) {
  if (error) return <p className="rounded-md bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>;
  if (isLoading) return <div className="h-64 animate-pulse rounded-2xl border border-line bg-surface-2" />;
  return <>{children}</>;
}

export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
}: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex gap-1 rounded-xl border border-line p-1">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={value === option.value}
          onClick={() => onChange(option.value)}
          className={`rounded-lg px-3 py-1.5 text-xs font-semibold ${value === option.value ? "bg-accent text-accent-ink" : "text-fg-muted hover:bg-surface-2"}`}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
