import { LayoutGrid } from "lucide-react";
import { categoryIcon } from "../../utils/categoryIcon";
import type { Category, Item } from "../../types";

interface CategoryPillsProps {
  categories: Category[];
  items: Item[];
  selectedCategoryId: number | null;
  onSelect: (categoryId: number | null) => void;
}

/** Compact category tabs pair an icon with the name and item count.
 * Icons are resolved from client-defined category names. */
export function CategoryPills({ categories, items, selectedCategoryId, onSelect }: CategoryPillsProps) {
  const countFor = (categoryId: number | null) =>
    categoryId === null ? items.length : items.filter((item) => item.categoryId === categoryId).length;

  const cardClass = (active: boolean) =>
    [
      "grid shrink-0 grid-cols-[auto_auto] items-center gap-x-3 gap-y-1 rounded-xl border px-3.5 py-3 transition-colors",
      active
        ? "border-accent/40 bg-accent-soft"
        : "border-line bg-surface hover:border-line-strong hover:bg-surface-2",
    ].join(" ");

  const badgeClass = (active: boolean) =>
    [
      "row-span-2 flex h-10 w-10 items-center justify-center rounded-xl transition-colors",
      active ? "bg-accent text-accent-ink" : "bg-surface-3 text-fg-muted",
    ].join(" ");

  return (
    <div className="flex gap-2.5 overflow-x-auto py-1">
      <button type="button" aria-pressed={selectedCategoryId === null} onClick={() => onSelect(null)} className={cardClass(selectedCategoryId === null)}>
        <span className={badgeClass(selectedCategoryId === null)}>
          <LayoutGrid className="h-5 w-5" />
        </span>
        <span className={`text-left text-xs font-semibold ${selectedCategoryId === null ? "text-accent-text" : "text-fg-2"}`}>
          All Menu
        </span>
        <span className={selectedCategoryId === null ? "text-left text-[11px] text-accent-text" : "text-left text-[11px] text-fg-muted"}>
          {countFor(null)} items
        </span>
      </button>

      {categories.map((category) => {
        const active = selectedCategoryId === category.id;
        const Icon = categoryIcon(category.name);
        return (
          <button key={category.id} type="button" aria-pressed={active} onClick={() => onSelect(category.id)} className={cardClass(active)}>
            <span className={badgeClass(active)}>
              <Icon className="h-5 w-5" />
            </span>
            <span className={`text-left text-xs font-semibold ${active ? "text-accent-text" : "text-fg-2"}`}>
              {category.name}
            </span>
            <span className={active ? "text-left text-[11px] text-accent-text" : "text-left text-[11px] text-fg-muted"}>
              {countFor(category.id)} items
            </span>
          </button>
        );
      })}
    </div>
  );
}
