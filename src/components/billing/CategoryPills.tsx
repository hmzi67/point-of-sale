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
      "grid shrink-0 grid-cols-[auto_auto] items-center gap-x-2.5 gap-y-0.5 rounded-xl border px-3 py-2 transition-colors",
      active
        ? "border-accent/40 bg-accent-soft"
        : "border-line bg-surface hover:border-line-strong hover:bg-surface-2",
    ].join(" ");

  const badgeClass = (active: boolean) =>
    [
      "row-span-2 flex h-8 w-8 items-center justify-center rounded-lg transition-colors",
      active ? "bg-accent text-accent-ink" : "bg-surface-3 text-fg-muted",
    ].join(" ");

  return (
    <div className="flex gap-2 overflow-x-auto py-0.5 scrollbar-none [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
      <button type="button" aria-pressed={selectedCategoryId === null} onClick={() => onSelect(null)} className={cardClass(selectedCategoryId === null)}>
        <span className={badgeClass(selectedCategoryId === null)}>
          <LayoutGrid className="h-4 w-4" />
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
              <Icon className="h-4 w-4" />
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
