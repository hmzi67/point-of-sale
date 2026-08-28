import { Pencil, Trash2 } from "lucide-react";
import type { TableSummary } from "../../types";

interface TableCardProps {
  table: TableSummary;
  busy: boolean;
  onSeat: () => void;
  onResume: () => void;
  onRelease: () => void;
  onReserve: () => void;
  onShift: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

/** Color-codes a table by status: free = green, occupied = red, reserved =
 * amber — the floor view's whole point is reading this at a glance. */
const STATUS_STYLES: Record<TableSummary["status"], string> = {
  free: "border-emerald-300 bg-emerald-50 hover:bg-emerald-100",
  occupied: "border-red-300 bg-red-50 hover:bg-red-100",
  reserved: "border-amber-300 bg-amber-50 hover:bg-amber-100",
};

const STATUS_DOT: Record<TableSummary["status"], string> = {
  free: "bg-emerald-500",
  occupied: "bg-red-500",
  reserved: "bg-amber-500",
};

const STATUS_LABEL: Record<TableSummary["status"], string> = {
  free: "Free",
  occupied: "Occupied",
  reserved: "Reserved",
};

/** A single table on the floor. Tapping a free or reserved table seats it
 * (starts a new order) and opens billing; tapping an occupied table resumes
 * its in-progress cart. Both go through the same primary action button so
 * the whole card is one big, cashier-friendly tap target. */
export function TableCard({
  table,
  busy,
  onSeat,
  onResume,
  onRelease,
  onReserve,
  onShift,
  onEdit,
  onDelete,
}: TableCardProps) {
  const primaryAction = table.status === "occupied" ? onResume : onSeat;

  return (
    <div
      className={`flex flex-col gap-2 rounded-lg border-2 p-4 transition-colors ${STATUS_STYLES[table.status]}`}
    >
      <div className="flex items-start justify-between gap-2">
        <button
          type="button"
          onClick={primaryAction}
          disabled={busy}
          className="flex flex-1 flex-col items-start gap-1 text-left disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span className="flex items-center gap-1.5 text-sm font-semibold text-slate-900">
            <span className={`h-2 w-2 rounded-full ${STATUS_DOT[table.status]}`} />
            {table.name}
          </span>
          <span className="text-xs text-slate-500">
            {table.seats} seat{table.seats === 1 ? "" : "s"} · {STATUS_LABEL[table.status]}
            {table.hasParkedOrder ? " · order in progress" : ""}
          </span>
        </button>

        <div className="flex shrink-0 gap-1">
          <button
            type="button"
            onClick={onEdit}
            disabled={busy}
            aria-label={`Rename ${table.name}`}
            title="Rename table"
            className="rounded p-1 text-slate-400 hover:bg-white hover:text-slate-600 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Pencil className="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onClick={onDelete}
            disabled={busy || table.hasParkedOrder}
            aria-label={`Delete ${table.name}`}
            title={table.hasParkedOrder ? "Clear the table's order before deleting it" : "Delete table"}
            className="rounded p-1 text-slate-400 hover:bg-white hover:text-red-600 disabled:cursor-not-allowed disabled:opacity-50"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      <div className="flex gap-1.5 text-xs">
        {table.status === "free" && (
          <button
            type="button"
            onClick={onReserve}
            disabled={busy}
            className="rounded border border-slate-300 bg-white/60 px-2 py-1 font-medium text-slate-600 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            Mark reserved
          </button>
        )}
        {table.status !== "free" && (
          <button
            type="button"
            onClick={onRelease}
            disabled={busy}
            className="rounded border border-slate-300 bg-white/60 px-2 py-1 font-medium text-slate-600 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            Clear table
          </button>
        )}
        {table.status === "occupied" && (
          <button
            type="button"
            onClick={onShift}
            disabled={busy}
            className="rounded border border-slate-300 bg-white/60 px-2 py-1 font-medium text-slate-600 hover:bg-white disabled:cursor-not-allowed disabled:opacity-50"
          >
            Shift table
          </button>
        )}
      </div>
    </div>
  );
}
