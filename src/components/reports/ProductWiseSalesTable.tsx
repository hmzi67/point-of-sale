import { useState } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { formatMinor } from "../../utils/format";
import type { Category, ProductSalesSummaryReport, TopItemSort } from "../../types";

interface ProductWiseSalesTableProps {
  data: ProductSalesSummaryReport | null;
  categories: Category[];
  categoryId: number | null;
  onCategoryChange: (categoryId: number | null) => void;
  sortBy: TopItemSort;
  onSortChange: (sortBy: TopItemSort) => void;
  currency: string;
  isLoading: boolean;
}

/** Rows per page for both the ranked and the no-sales sections — a plain
 * client-side slice (the report is already fully fetched) rather than a
 * server-paginated query, since a shop's whole catalog is a bounded,
 * hundreds-of-rows list, not something that needs streaming from SQLite. */
const PAGE_SIZE = 25;

function Pager({ page, pageCount, onChange }: { page: number; pageCount: number; onChange: (page: number) => void }) {
  if (pageCount <= 1) return null;
  return (
    <div className="flex items-center justify-end gap-2 border-t border-slate-100 px-4 py-2 text-xs text-slate-500">
      <button
        type="button"
        onClick={() => onChange(Math.max(0, page - 1))}
        disabled={page === 0}
        className="rounded p-1 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-40"
        aria-label="Previous page"
      >
        <ChevronLeft className="h-3.5 w-3.5" />
      </button>
      <span>
        Page {page + 1} of {pageCount}
      </span>
      <button
        type="button"
        onClick={() => onChange(Math.min(pageCount - 1, page + 1))}
        disabled={page >= pageCount - 1}
        className="rounded p-1 hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-40"
        aria-label="Next page"
      >
        <ChevronRight className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

export function ProductWiseSalesTable({
  data,
  categories,
  categoryId,
  onCategoryChange,
  sortBy,
  onSortChange,
  currency,
  isLoading,
}: ProductWiseSalesTableProps) {
  const [soldPage, setSoldPage] = useState(0);
  const [noSalesPage, setNoSalesPage] = useState(0);
  const [showNoSales, setShowNoSales] = useState(false);

  const rows = data?.rows ?? [];
  const noSalesItems = data?.noSalesItems ?? [];
  const soldPageCount = Math.max(1, Math.ceil(rows.length / PAGE_SIZE));
  const noSalesPageCount = Math.max(1, Math.ceil(noSalesItems.length / PAGE_SIZE));
  const visibleRows = rows.slice(soldPage * PAGE_SIZE, soldPage * PAGE_SIZE + PAGE_SIZE);
  const visibleNoSales = noSalesItems.slice(noSalesPage * PAGE_SIZE, noSalesPage * PAGE_SIZE + PAGE_SIZE);

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <select
          value={categoryId ?? ""}
          onChange={(e) => onCategoryChange(e.target.value ? Number(e.target.value) : null)}
          className="rounded-md border border-slate-300 px-3 py-1.5 text-sm"
        >
          <option value="">All categories</option>
          {categories.map((category) => (
            <option key={category.id} value={category.id}>
              {category.name}
            </option>
          ))}
        </select>

        <div className="flex rounded-md border border-slate-300 p-0.5">
          {(["revenue", "quantity"] as const).map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => onSortChange(s)}
              className={[
                "rounded px-3 py-1 text-sm font-medium transition-colors",
                sortBy === s ? "bg-brand-600 text-white" : "text-slate-600 hover:bg-slate-100",
              ].join(" ")}
            >
              {s === "revenue" ? "Sort by Revenue" : "Sort by Quantity"}
            </button>
          ))}
        </div>
      </div>

      <div className="rounded-lg border border-slate-200 bg-white">
        <div className="border-b border-slate-200 px-4 py-3">
          <h3 className="text-sm font-semibold text-slate-900">Product Wise Sales</h3>
          <p className="mt-0.5 text-xs text-slate-500">Every item sold in the range, ranked by {sortBy}.</p>
        </div>

        {isLoading ? (
          <div className="space-y-2 p-4">
            {Array.from({ length: 5 }).map((_, i) => (
              <div key={i} className="h-8 animate-pulse rounded bg-slate-50" />
            ))}
          </div>
        ) : rows.length === 0 ? (
          <p className="px-4 py-10 text-center text-sm text-slate-400">No sales in this range</p>
        ) : (
          <>
            <table className="w-full text-left text-sm">
              <thead className="border-b border-slate-100 text-xs uppercase tracking-wide text-slate-500">
                <tr>
                  <th className="px-4 py-2 font-medium">#</th>
                  <th className="px-4 py-2 font-medium">Item</th>
                  <th className="px-4 py-2 font-medium">Category</th>
                  <th className="px-4 py-2 text-right font-medium">Qty Sold</th>
                  <th className="px-4 py-2 text-right font-medium">Revenue</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100">
                {visibleRows.map((row) => (
                  <tr key={row.itemId}>
                    <td className="px-4 py-2 text-slate-400">{row.rank}</td>
                    <td className="px-4 py-2 font-medium text-slate-900">{row.itemName}</td>
                    <td className="px-4 py-2 text-slate-600">{row.categoryName}</td>
                    <td className="px-4 py-2 text-right text-slate-700">{row.qtySold}</td>
                    <td className="px-4 py-2 text-right text-slate-900">{formatMinor(row.revenueMinor, currency)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <Pager page={soldPage} pageCount={soldPageCount} onChange={setSoldPage} />
          </>
        )}
      </div>

      {!isLoading && noSalesItems.length > 0 && (
        <div className="rounded-lg border border-slate-200 bg-white">
          <button
            type="button"
            onClick={() => setShowNoSales((s) => !s)}
            className="flex w-full items-center justify-between px-4 py-3 text-left"
          >
            <div>
              <h3 className="text-sm font-semibold text-slate-900">No Sales This Period</h3>
              <p className="mt-0.5 text-xs text-slate-500">
                {noSalesItems.length} item{noSalesItems.length === 1 ? "" : "s"} with zero sales in this range — slow-moving stock worth a look.
              </p>
            </div>
            <span className="text-xs font-medium text-brand-600">{showNoSales ? "Hide" : "Show"}</span>
          </button>

          {showNoSales && (
            <>
              <table className="w-full border-t border-slate-100 text-left text-sm">
                <thead className="border-b border-slate-100 text-xs uppercase tracking-wide text-slate-500">
                  <tr>
                    <th className="px-4 py-2 font-medium">Item</th>
                    <th className="px-4 py-2 font-medium">Category</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-100">
                  {visibleNoSales.map((row) => (
                    <tr key={row.itemId}>
                      <td className="px-4 py-2 font-medium text-slate-900">{row.itemName}</td>
                      <td className="px-4 py-2 text-slate-600">{row.categoryName}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <Pager page={noSalesPage} pageCount={noSalesPageCount} onChange={setNoSalesPage} />
            </>
          )}
        </div>
      )}
    </div>
  );
}
