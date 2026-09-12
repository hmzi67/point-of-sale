import { Flame, PackageX, TrendingDown, X } from "lucide-react";

export interface InventoryFilters {
  lowStock: boolean;
  outOfStock: boolean;
  bestSeller: boolean;
  minPrice: string;
  maxPrice: string;
}

export const EMPTY_INVENTORY_FILTERS: InventoryFilters = {
  lowStock: false,
  outOfStock: false,
  bestSeller: false,
  minPrice: "",
  maxPrice: "",
};

interface InventoryFilterChipsProps {
  filters: InventoryFilters;
  onChange: (filters: InventoryFilters) => void;
}

const TOGGLE_CHIPS: Array<{ key: "lowStock" | "outOfStock" | "bestSeller"; label: string; icon: typeof Flame }> = [
  { key: "lowStock", label: "Low stock", icon: TrendingDown },
  { key: "outOfStock", label: "Out of stock", icon: PackageX },
  { key: "bestSeller", label: "Best sellers", icon: Flame },
];

/**
 * Independent, combinable filter toggles ("low stock" + "a category" can
 * both be active at once — they AND together, and with the toolbar's
 * existing search/category filter) plus a simple min/max price range.
 * Everything here filters the already-fetched `items` list client-side
 * (see `InventoryPage`) rather than adding server-side query params — a
 * shop's catalog is a bounded, hundreds-of-rows list, not something that
 * needs its own indexed SQL predicate per toggle.
 */
export function InventoryFilterChips({ filters, onChange }: InventoryFilterChipsProps) {
  const toggle = (key: "lowStock" | "outOfStock" | "bestSeller") => {
    onChange({ ...filters, [key]: !filters[key] });
  };

  const hasActiveFilters =
    filters.lowStock || filters.outOfStock || filters.bestSeller || filters.minPrice !== "" || filters.maxPrice !== "";

  return (
    <div className="flex flex-wrap items-center gap-2">
      {TOGGLE_CHIPS.map(({ key, label, icon: Icon }) => {
        const active = filters[key];
        return (
          <button
            key={key}
            type="button"
            onClick={() => toggle(key)}
            aria-pressed={active}
            className={[
              "flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors",
              active
                ? "border-accent/40 bg-accent-soft text-accent-text"
                : "border-line bg-surface text-fg-2 hover:bg-surface-2",
            ].join(" ")}
          >
            <Icon className="h-3.5 w-3.5" />
            {label}
          </button>
        );
      })}

      <div className="flex items-center gap-1.5 rounded-lg border border-line bg-surface px-3 py-1 text-xs">
        <span className="text-fg-muted">Price</span>
        <input
          type="number"
          min={0}
          inputMode="decimal"
          value={filters.minPrice}
          onChange={(e) => onChange({ ...filters, minPrice: e.target.value })}
          aria-label="Minimum price"
          placeholder="Min"
          className="w-16 rounded border-0 bg-transparent p-0 text-xs focus:outline-none focus:ring-0"
        />
        <span className="text-fg-faint">–</span>
        <input
          type="number"
          min={0}
          inputMode="decimal"
          value={filters.maxPrice}
          onChange={(e) => onChange({ ...filters, maxPrice: e.target.value })}
          aria-label="Maximum price"
          placeholder="Max"
          className="w-16 rounded border-0 bg-transparent p-0 text-xs focus:outline-none focus:ring-0"
        />
      </div>

      {hasActiveFilters && (
        <button
          type="button"
          onClick={() => onChange(EMPTY_INVENTORY_FILTERS)}
          className="flex items-center gap-1 rounded-lg px-2 py-1.5 text-xs font-medium text-fg-muted hover:bg-surface-3"
        >
          <X className="h-3.5 w-3.5" />
          Clear filters
        </button>
      )}
    </div>
  );
}
