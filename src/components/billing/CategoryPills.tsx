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
        ? "border-accent/40 bg-accent-soft"
        : "border-transparent bg-surface shadow-soft hover:bg-surface-2",
    ].join(" ");

  const badgeClass = (active: boolean) =>
    [
      "flex h-11 w-11 items-center justify-center rounded-full transition-colors",
      active ? "bg-accent text-accent-ink" : "bg-surface-3 text-fg-muted",
    ].join(" ");

  return (
    <div className="flex gap-2.5 overflow-x-auto pb-1">
      <button type="button" onClick={() => onSelect(null)} className={cardClass(selectedCategoryId === null)}>
        <span className={badgeClass(selectedCategoryId === null)}>
          <LayoutGrid className="h-5 w-5" />
        </span>
        <span className={`text-xs font-semibold ${selectedCategoryId === null ? "text-accent-text" : "text-fg-2"}`}>
          All Menu
        </span>
        <span className={selectedCategoryId === null ? "text-[11px] text-accent-text" : "text-[11px] text-fg-muted"}>
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
            <span className={`text-xs font-semibold ${active ? "text-accent-text" : "text-fg-2"}`}>
              {category.name}
            </span>
            <span className={active ? "text-[11px] text-accent-text" : "text-[11px] text-fg-muted"}>
              {countFor(category.id)} items
            </span>
          </button>
        );
      })}
    </div>
  );
}
