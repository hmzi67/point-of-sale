import { Plus, Search, Upload } from "lucide-react";
import { useInventoryStore } from "../../store";

interface InventoryToolbarProps {
  isReadOnly: boolean;
  onAddItem: () => void;
  onImportCsv: () => void;
}

export function InventoryToolbar({ isReadOnly, onAddItem, onImportCsv }: InventoryToolbarProps) {
  const search = useInventoryStore((state) => state.search);
  const setSearch = useInventoryStore((state) => state.setSearch);
  const categoryId = useInventoryStore((state) => state.categoryId);
  const setCategoryId = useInventoryStore((state) => state.setCategoryId);
  const categories = useInventoryStore((state) => state.categories);

  return (
    <div className="flex flex-wrap items-center gap-3">
      <div className="relative min-w-[220px] flex-1">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-400" />
        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search by name or scan barcode…"
          className="w-full rounded-md border border-slate-300 py-2 pl-9 pr-3 text-sm"
        />
      </div>

      <select
        value={categoryId ?? ""}
        onChange={(e) => setCategoryId(e.target.value ? Number(e.target.value) : null)}
        className="rounded-md border border-slate-300 px-3 py-2 text-sm"
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
          <button
            type="button"
            onClick={onImportCsv}
            className="flex items-center gap-1.5 rounded-md border border-slate-300 px-3 py-2 text-sm font-medium text-slate-700 hover:bg-slate-50"
          >
            <Upload className="h-4 w-4" />
            Import CSV
          </button>
          <button
            type="button"
            onClick={onAddItem}
            className="flex items-center gap-1.5 rounded-md bg-brand-600 px-3 py-2 text-sm font-medium text-white hover:bg-brand-700"
          >
            <Plus className="h-4 w-4" />
            Add item
          </button>
        </>
      )}
    </div>
  );
}
