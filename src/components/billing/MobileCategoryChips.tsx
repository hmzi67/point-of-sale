import { Flame, LayoutGrid } from "lucide-react";
import type { Category } from "../../types";

/** `"best-seller"` is a pseudo-category, not a real `category_id` — selecting
 * it filters the grid down to whatever `useBestSellerIds` currently returns
 * (live sales data), the same way selecting a real category filters by
 * `categoryId`. Single-select, same as the desktop `CategoryPills`. */
export type MobileCategorySelection = number | null | "best-seller";

interface MobileCategoryChipsProps {
  categories: Category[];
  selected: MobileCategorySelection;
  onSelect: (selection: MobileCategorySelection) => void;
}

/**
 * A compact chip row — the mobile-appropriate counterpart to desktop's
 * bigger icon-badge `CategoryPills` cards, matching the reference's mobile
 * layout convention of small pill filters rather than large tappable cards.
 */
export function MobileCategoryChips({ categories, selected, onSelect }: MobileCategoryChipsProps) {
  const chipClass = (active: boolean) =>
    [
      "flex shrink-0 items-center gap-1.5 rounded-full border px-3.5 py-1.5 text-xs font-semibold transition-colors",
      active ? "border-brand-600 bg-brand-600 text-white" : "border-slate-200 bg-white text-slate-600",
    ].join(" ");

  return (
    <div className="flex gap-2 overflow-x-auto pb-1">
      <button type="button" onClick={() => onSelect(null)} className={chipClass(selected === null)}>
        <LayoutGrid className="h-3.5 w-3.5" />
        All
      </button>

      <button
        type="button"
        onClick={() => onSelect("best-seller")}
        className={chipClass(selected === "best-seller")}
      >
        <Flame className={`h-3.5 w-3.5 ${selected === "best-seller" ? "text-white" : "text-orange-500"}`} />
        Best Sellers
      </button>

      {categories.map((category) => (
        <button
          key={category.id}
          type="button"
          onClick={() => onSelect(category.id)}
          className={chipClass(selected === category.id)}
        >
          {category.name}
        </button>
      ))}
    </div>
  );
}
