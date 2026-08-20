import { LayoutGrid } from "lucide-react";
import { categoryIcon } from "../../utils/categoryIcon";
import type { Category, Item } from "../../types";

interface CategoryPillsProps {
  categories: Category[];
  items: Item[];
  selectedCategoryId: number | null;
  onSelect: (categoryId: number | null) => void;
}

/** Icon-badge-on-top, name, item-count card per category (plus "All Menu")
 * — not a text pill/tab. Active state fills the badge solid brand color
 * with a white icon and tints the whole card; inactive stays neutral gray.
 * Categories are entirely client-defined, so the icon is resolved from the
 * name via `categoryIcon` rather than any fixed per-category list — see
 * that util's doc comment. */
export function CategoryPills({ categories, items, selectedCategoryId, onSelect }: CategoryPillsProps) {
  const countFor = (categoryId: number | null) =>
    categoryId === null ? items.length : items.filter((item) => item.categoryId === categoryId).length;

  const cardClass = (active: boolean) =>
    [
      "flex shrink-0 flex-col items-center gap-2 rounded-2xl border px-4 py-3.5 transition-colors",
      active
        ? "border-brand-200 bg-brand-50"
        : "border-transparent bg-white shadow-soft hover:bg-slate-50",
    ].join(" ");

  const badgeClass = (active: boolean) =>
    [
      "flex h-11 w-11 items-center justify-center rounded-full transition-colors",
      active ? "bg-brand-600 text-white" : "bg-slate-100 text-slate-500",
    ].join(" ");

  return (
    <div className="flex gap-2.5 overflow-x-auto pb-1">
      <button type="button" onClick={() => onSelect(null)} className={cardClass(selectedCategoryId === null)}>
        <span className={badgeClass(selectedCategoryId === null)}>
          <LayoutGrid className="h-5 w-5" />
        </span>
        <span className={`text-xs font-semibold ${selectedCategoryId === null ? "text-brand-700" : "text-slate-700"}`}>
          All Menu
        </span>
        <span className={selectedCategoryId === null ? "text-[11px] text-brand-500" : "text-[11px] text-slate-400"}>
          {countFor(null)} items
        </span>
      </button>

      {categories.map((category) => {
        const active = selectedCategoryId === category.id;
        const Icon = categoryIcon(category.name);
        return (
          <button key={category.id} type="button" onClick={() => onSelect(category.id)} className={cardClass(active)}>
            <span className={badgeClass(active)}>
              <Icon className="h-5 w-5" />
            </span>
            <span className={`text-xs font-semibold ${active ? "text-brand-700" : "text-slate-700"}`}>
              {category.name}
            </span>
            <span className={active ? "text-[11px] text-brand-500" : "text-[11px] text-slate-400"}>
              {countFor(category.id)} items
            </span>
          </button>
        );
      })}
    </div>
  );
}
