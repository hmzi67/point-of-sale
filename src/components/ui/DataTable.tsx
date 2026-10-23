import type { ReactNode } from "react";

/** A bordered, horizontally scrollable table with a muted header row. */
export function DataTable({ head, children }: { head: ReactNode[]; children: ReactNode }) {
  return (
    <div className="overflow-x-auto rounded-2xl border border-line bg-surface">
      <table className="w-full text-sm">
        <thead className="border-b border-line text-left text-xs text-fg-muted">
          <tr>
            {head.map((h, i) => (
              <th key={i} className="whitespace-nowrap px-4 py-3 font-medium">{h}</th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-line">{children}</tbody>
      </table>
    </div>
  );
}

export function EmptyRow({ span, text }: { span: number; text: string }) {
  return (
    <tr>
      <td colSpan={span} className="px-4 py-10 text-center text-sm text-fg-muted">{text}</td>
    </tr>
  );
}
