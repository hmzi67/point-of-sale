import { Barcode, ClipboardCheck, History, Plus, Search, Upload } from "lucide-react";
import { upgradeHint, useCapability } from "../../hooks/useCapability";
import { useInventoryStore } from "../../store";

interface InventoryToolbarProps {
  isReadOnly: boolean;
  onAddItem: () => void;
  onImportCsv: () => void;
  onStockTake: () => void;
  onMovements: () => void;
  onLabels: () => void;
}

const toolClass =
  "flex items-center gap-1.5 rounded-xl border border-line bg-surface-2/50 px-3 py-3 text-sm font-medium text-fg-2 hover:bg-surface-2 disabled:cursor-not-allowed disabled:opacity-50";

export function InventoryToolbar({ isReadOnly, onAddItem, onImportCsv, onStockTake, onMovements, onLabels }: InventoryToolbarProps) {
  const search = useInventoryStore((state) => state.search);
  const setSearch = useInventoryStore((state) => state.setSearch);
  const categoryId = useInventoryStore((state) => state.categoryId);
  const setCategoryId = useInventoryStore((state) => state.setCategoryId);
  // Stock takes, labels and CSV import are Full Inventory (a plan feature).
  const advanced = useCapability("inventory_advanced");
  const advancedTitle = advanced.allowed ? undefined : upgradeHint(advanced.planName, advanced.requiredPlan);
  const categories = useInventoryStore((state) => state.categories);

  return (
    <div className="flex flex-wrap items-center gap-3">
      <div className="relative min-w-0 basis-64 flex-1">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          aria-label="Search inventory"
          placeholder="Search products by name or barcode…"
          className="w-full rounded-xl border border-line bg-surface-2/50 py-3 pl-10 pr-3 text-sm placeholder:text-fg-muted focus:border-accent/40 focus:outline-none focus:ring-2 focus:ring-accent/15"
        />
      </div>

      <select
        aria-label="Filter by category"
        value={categoryId ?? ""}
        onChange={(e) => setCategoryId(e.target.value ? Number(e.target.value) : null)}
        className="rounded-xl border border-line bg-surface-2/50 px-3 py-3 text-sm"
      >
        <option value="">All categories</option>
        {categories.map((category) => (
          <option key={category.id} value={category.id}>
            {category.name}
          </option>
        ))}
      </select>

      {!isReadOnly && (
        <>
          <button type="button" onClick={onStockTake} className={toolClass} disabled={!advanced.allowed} title={advancedTitle}>
            <ClipboardCheck className="h-4 w-4" />
            Stock take
          </button>
          <button type="button" onClick={onMovements} className={toolClass}>
            <History className="h-4 w-4" />
            Movements
          </button>
          <button type="button" onClick={onLabels} className={toolClass} disabled={!advanced.allowed} title={advancedTitle}>
            <Barcode className="h-4 w-4" />
            Labels
          </button>
          <button
            type="button"
            onClick={onImportCsv}
            disabled={!advanced.allowed}
            title={advancedTitle}
            className={toolClass}
          >
            <Upload className="h-4 w-4" />
            Import CSV
          </button>
          <button
            type="button"
            onClick={onAddItem}
            className="flex items-center gap-2 rounded-xl bg-accent px-4 py-3 text-sm font-medium text-accent-ink hover:bg-accent-hover"
          >
            <Plus className="h-4 w-4" />
            Add item
          </button>
        </>
      )}
    </div>
  );
}
