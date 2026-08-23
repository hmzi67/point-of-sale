import { PackageSearch } from "lucide-react";
import { ItemCard } from "./ItemCard";
import type { Item } from "../../types";

interface ItemGridProps {
  items: Item[];
  currency: string;
  isLoading: boolean;
  /** True while a search query is driving `items` (see BillingPage) — only
   * changes the empty-state copy, so "no matches" doesn't read as "this
   * category is empty". */
  isSearching?: boolean;
  /** Item ids currently qualifying as a "best seller" (see
   * `useBestSellerIds`) — items in this set get a fire badge on their card. */
  bestSellerIds?: Set<number>;
  /** Android's grid stays a fixed 2 columns regardless of viewport width
   * (a tablet included) — the reference's mobile layout convention, as
   * distinct from desktop's width-responsive 2/3/4-column grid. */
  fixedTwoColumns?: boolean;
}

export function ItemGrid({
  items,
  currency,
  isLoading,
  isSearching = false,
  bestSellerIds,
  fixedTwoColumns = false,
}: ItemGridProps) {
  const gridClass = fixedTwoColumns ? "grid grid-cols-2 gap-3" : "grid grid-cols-2 gap-4 sm:grid-cols-3 xl:grid-cols-4";

  if (isLoading) {
    return (
      <div className={gridClass}>
        {Array.from({ length: 8 }).map((_, i) => (
          <div key={i} className="h-52 animate-pulse rounded-2xl bg-white shadow-soft" />
        ))}
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 py-16 text-slate-400">
        <PackageSearch className="h-8 w-8" />
        <p className="text-sm">{isSearching ? "No items match your search" : "No items in this category yet"}</p>
      </div>
    );
  }

  return (
    <div className={gridClass}>
      {items.map((item) => (
        <ItemCard key={item.id} item={item} currency={currency} isBestSeller={bestSellerIds?.has(item.id) ?? false} />
      ))}
    </div>
  );
}
