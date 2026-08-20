import { PackageSearch } from "lucide-react";
import { ItemCard } from "./ItemCard";
import type { Item } from "../../types";

interface ItemGridProps {
  items: Item[];
  currency: string;
  isLoading: boolean;
  onOpenItem: (item: Item) => void;
}

export function ItemGrid({ items, currency, isLoading, onOpenItem }: ItemGridProps) {
  if (isLoading) {
    return (
      <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 xl:grid-cols-4">
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
        <p className="text-sm">No items in this category yet</p>
      </div>
    );
  }

  return (
    <div className="grid grid-cols-2 gap-4 sm:grid-cols-3 xl:grid-cols-4">
      {items.map((item) => (
        <ItemCard key={item.id} item={item} currency={currency} onOpen={onOpenItem} />
      ))}
    </div>
  );
}
