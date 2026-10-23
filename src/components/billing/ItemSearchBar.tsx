import { useEffect, useRef, useState } from "react";
import { Search } from "lucide-react";
import { searchItems } from "../../services/billingService";
import { lookupScaleBarcode } from "../../services/stockService";
import { useDebouncedValue } from "../../hooks/useDebouncedValue";
import { useBillingStore } from "../../store";
import type { Item } from "../../types";

interface ItemSearchBarProps {
  /** Fired whenever the (debounced) search settles — with the trimmed
   * query and its full-catalog results. `query === ""` means "search
   * cleared"; the caller (BillingPage) uses that to fall back to whatever
   * category pill is selected, and treats a non-empty query as overriding
   * the category filter entirely, matching the item grid to these same
   * results. */
  onResultsChange: (query: string, results: Item[]) => void;
}

/**
 * The billing screen's only mandatory input: typing filters the item grid
 * live (via `onResultsChange`, always against the whole catalog — never
 * scoped to the currently selected category), and a barcode scan — which
 * types the code then sends Enter, indistinguishable from fast typing —
 * adds the top match straight to the cart with no mouse involved. Search
 * state (query, results) is local to this component rather than in the
 * billing store, so typing here never re-renders the cart panel, and the
 * cart's own updates never re-render this.
 */
export function ItemSearchBar({ onResultsChange }: ItemSearchBarProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Item[]>([]);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const requestIdRef = useRef(0);

  const addItem = useBillingStore((state) => state.addItem);
  const addItemQty = useBillingStore((state) => state.addItemQty);

  // Debounced so fast typing (or a barcode scanner, which types character by
  // character) doesn't fire a search per keystroke — only once typing pauses.
  const debouncedQuery = useDebouncedValue(query, 120);

  useEffect(() => {
    const trimmed = debouncedQuery.trim();
    if (!trimmed) {
      setResults([]);
      setError(null);
      onResultsChange("", []);
      return;
    }

    const requestId = ++requestIdRef.current;
    searchItems(trimmed)
      .then((items) => {
        // A slower, earlier request must never clobber a newer result set.
        if (requestId !== requestIdRef.current) return;
        setResults(items);
        setError(null);
        onResultsChange(trimmed, items);
      })
      .catch((e: Error) => {
        if (requestId !== requestIdRef.current) return;
        setError(e.message);
      });
    // Deliberately omits onResultsChange: it's a fresh closure from
    // BillingPage every render, and only the settled query should
    // re-trigger this search.
  }, [debouncedQuery]);

  const addAndReset = (item: Item) => {
    if (item.stockQty <= 0) {
      setError(`${item.name} is out of stock`);
      return;
    }
    addItem(item);
    setQuery("");
    setResults([]);
    setError(null);
    onResultsChange("", []);
    inputRef.current?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      // A scanner's Enter arrives right after its digits; results may not
      // have caught up with the very latest keystroke yet, but the exact
      // barcode match is always sorted first by the backend, so acting on
      // whatever the current top result is remains correct.
      //
      // A 13-digit code that isn't an item's own barcode may be a
      // weight-scale label (item code + weight/price) — Rust decodes it
      // against the configured layout, or says it isn't one.
      const top = results[0];
      const code = query.trim();
      if (/^\d{13}$/.test(code) && top?.barcode !== code) {
        lookupScaleBarcode(code)
          .then((match) => {
            if (!match) {
              if (top) addAndReset(top);
              else setError(`No item matches ${code}`);
              return;
            }
            if (match.item.stockQty <= 0) {
              setError(`${match.item.name} is out of stock`);
              return;
            }
            addItemQty(match.item, match.qty);
            setQuery("");
            setResults([]);
            setError(null);
            onResultsChange("", []);
            inputRef.current?.focus();
          })
          .catch((e: Error) => setError(e.message));
        return;
      }
      if (top) addAndReset(top);
    } else if (event.key === "Escape") {
      setQuery("");
      setResults([]);
      onResultsChange("", []);
    }
  };

  return (
    <div className="relative">
      <input
        ref={inputRef}
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={onKeyDown}
        aria-label="Search products or scan a barcode"
        placeholder="Search products or scan a barcode…"
        className="w-full rounded-xl border border-line bg-surface py-3.5 pl-12 pr-4 text-sm placeholder:text-fg-muted focus:outline-none focus:ring-2 focus:ring-accent/40"
      />
      <span className="pointer-events-none absolute left-3 top-6 flex h-9 w-9 -translate-y-1/2 items-center justify-center rounded-lg">
        <Search className="h-4 w-4 text-fg-muted" />
      </span>

      {error && <p className="mt-1.5 text-sm text-danger-text">{error}</p>}
    </div>
  );
}
