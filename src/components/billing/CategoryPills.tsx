import { LayoutGrid } from "lucide-react";
import { categoryColor } from "../../utils/categoryColor";
import type { Category, Item } from "../../types";

interface CategoryPillsProps {
  categories: Category[];
  items: Item[];
  selectedCategoryId: number | null;
  onSelect: (categoryId: number | null) => void;
}

/** "All Menu" plus one pill per real category, each showing the item count
 * for that category — counted from the items actually loaded for this
 * screen, never hardcoded. */
export function CategoryPills({ categories, items, selectedCategoryId, onSelect }: CategoryPillsProps) {
  const countFor = (categoryId: number | null) =>
    categoryId === null ? items.length : items.filter((item) => item.categoryId === categoryId).length;

  return (
    <div className="flex gap-2.5 overflow-x-auto pb-1">
      <button
        type="button"
        onClick={() => onSelect(null)}
        className={[
          "flex shrink-0 flex-col items-center gap-1.5 rounded-2xl px-4 py-3 text-left transition-colors",
          selectedCategoryId === null
            ? "bg-brand-600 text-white shadow-soft"
            : "bg-white text-slate-700 shadow-soft hover:bg-slate-50",
        ].join(" ")}
      >
        <LayoutGrid className="h-5 w-5" />
        <span className="text-xs font-semibold">All Menu</span>
        <span className={selectedCategoryId === null ? "text-[11px] text-brand-100" : "text-[11px] text-slate-400"}>
          {countFor(null)} items
        </span>
      </button>

      {categories.map((category) => {
        const active = selectedCategoryId === category.id;
        const color = categoryColor(category.id);
        return (
          <button
            key={category.id}
            type="button"
            onClick={() => onSelect(category.id)}
            className={[
              "flex shrink-0 flex-col items-center gap-1.5 rounded-2xl px-4 py-3 text-left transition-colors",
              active ? "bg-brand-600 text-white shadow-soft" : "bg-white text-slate-700 shadow-soft hover:bg-slate-50",
            ].join(" ")}
          >
            <span
              className={[
                "flex h-5 w-5 items-center justify-center rounded-full text-[10px] font-bold",
                active ? "bg-white/25" : color.dot,
              ].join(" ")}
            />
            <span className="text-xs font-semibold">{category.name}</span>
            <span className={active ? "text-[11px] text-brand-100" : "text-[11px] text-slate-400"}>
              {countFor(category.id)} items
            </span>
          </button>
        );
      })}
    </div>
  );
}
