import { Armchair, Check } from "lucide-react";
import { ModalCard } from "../ui/ModalCard";
import { TableSeating } from "../tables/TableSeating";
import { groupByFloor } from "../../utils/tableFloors";
import type { TableSummary } from "../../types";

const STATUS_STYLES: Record<TableSummary["status"], string> = {
  free: "border-success-line text-success-text",
  occupied: "border-danger-line text-danger-text",
  reserved: "border-warning-line text-warning-text",
};

const STATUS_DOT: Record<TableSummary["status"], string> = {
  free: "bg-success",
  occupied: "bg-danger",
  reserved: "bg-warning",
};

const STATUS_LABEL: Record<TableSummary["status"], string> = {
  free: "Free",
  occupied: "Occupied",
  reserved: "Reserved",
};

interface TablePickerModalProps {
  tables: TableSummary[];
  selectedId: number | null;
  onSelect: (id: number) => void;
  onClose: () => void;
}

/** The billing screen's table chooser: every table as a tile with its chairs
 * drawn out, colour-coded by status — replaces a dropdown that was too
 * cramped on small Windows screens. */
export function TablePickerModal({ tables, selectedId, onSelect, onClose }: TablePickerModalProps) {
  const groups = groupByFloor(tables);
  const count = (status: TableSummary["status"]) => tables.filter((t) => t.status === status).length;

  return (
    <ModalCard
      title="Select table"
      subtitle={`${count("free")} free · ${count("occupied")} occupied · ${count("reserved")} reserved`}
      icon={Armchair}
      width="lg"
      onClose={onClose}
    >
      {tables.length === 0 ? (
        <p className="py-10 text-center text-sm text-fg-muted">No tables yet — add them under Tables.</p>
      ) : (
        <div className="space-y-4">
          {groups.map((group) => (
            <section key={group.floor ?? ""}>
              {groups.length > 1 && <h4 className="mb-2 text-xs font-semibold uppercase tracking-wider text-fg-muted">{group.floor ?? "Other tables"}</h4>}
              <div className="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-3">
                {group.tables.map((table) => {
                  const selected = table.id === selectedId;
                  return (
                    <button
                      key={table.id}
                      type="button"
                      onClick={() => onSelect(table.id)}
                      aria-pressed={selected}
                      aria-label={`${table.name}, ${table.seats} seats, ${STATUS_LABEL[table.status]}`}
                      className={[
                        "relative flex flex-col gap-1 rounded-xl border bg-surface p-3 text-left transition-colors hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-accent",
                        STATUS_STYLES[table.status],
                        selected ? "ring-2 ring-accent" : "",
                      ].join(" ")}
                    >
                      {selected && (
                        <span className="absolute right-2 top-2 flex h-5 w-5 items-center justify-center rounded-full bg-accent text-accent-ink">
                          <Check className="h-3.5 w-3.5" />
                        </span>
                      )}
                      <span className="flex items-center gap-1.5 pr-6 text-sm font-semibold text-fg">
                        <span className={`h-2 w-2 shrink-0 rounded-full ${STATUS_DOT[table.status]}`} />
                        <span className="truncate">{table.name}</span>
                      </span>
                      <TableSeating seats={table.seats} className="h-24 w-full" />
                      <span className="text-xs font-medium text-fg-muted">
                        {STATUS_LABEL[table.status]}
                        {table.hasParkedOrder ? " · order parked" : ""}
                      </span>
                    </button>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}
    </ModalCard>
  );
}
