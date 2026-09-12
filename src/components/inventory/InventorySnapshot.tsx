import { Boxes, PackageCheck, PackageX, TriangleAlert } from "lucide-react";
import type { Item } from "../../types";

/** Counts describe the currently visible products, including active filters. */
export function InventorySnapshot({ items, isLoading }: { items: Item[]; isLoading: boolean }) {
  const cards = [
    { label: "Products in view", value: items.length, detail: "Matching your current filters", icon: Boxes, tone: "bg-accent-soft text-accent-text", featured: true },
    { label: "Healthy stock", value: items.filter((item) => item.stockQty > 0 && !item.isLowStock).length, detail: "Above the low-stock threshold", icon: PackageCheck, tone: "bg-success-soft text-success-text" },
    { label: "Running low", value: items.filter((item) => item.stockQty > 0 && item.isLowStock).length, detail: "Available, but ready to restock", icon: TriangleAlert, tone: "bg-warning-soft text-warning-text" },
    { label: "Out of stock", value: items.filter((item) => item.stockQty <= 0).length, detail: "Products that need replenishing", icon: PackageX, tone: "bg-danger-soft text-danger-text" },
  ];
  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
      {cards.map(({ label, value, detail, icon: Icon, tone, featured }) => (
        <div key={label} className={`rounded-2xl border p-5 ${featured ? "border-accent/25 bg-linear-to-br from-accent-soft to-surface" : "border-line bg-surface"}`}>
          <div className="flex items-center justify-between gap-3">
            <p className="text-xs font-medium text-fg-muted">{label}</p>
            <span className={`flex h-10 w-10 items-center justify-center rounded-xl ${tone}`}><Icon className="h-5 w-5" strokeWidth={1.75} /></span>
          </div>
          <p className="mt-3 text-3xl font-bold tracking-tight text-fg tabular-nums">{isLoading ? <span className="inline-block h-9 w-16 animate-pulse rounded bg-surface-3" /> : value}</p>
          <p className="mt-2 text-[11px] text-fg-muted">{detail}</p>
        </div>
      ))}
    </div>
  );
}
