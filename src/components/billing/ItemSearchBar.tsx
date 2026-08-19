import { useEffect, useRef, useState } from "react";
import { Search } from "lucide-react";
import { searchItems } from "../../services/billingService";
import { useDebouncedValue } from "../../hooks/useDebouncedValue";
import { useBillingStore } from "../../store";
import { formatMinor } from "../../utils/format";
import type { Item } from "../../types";

interface ItemSearchBarProps {
  currency: string;
}

/**
 * The billing screen's only mandatory input: typing filters items live, and
 * a barcode scan — which types the code then sends Enter, indistinguishable
 * from fast typing — adds the top match straight to the cart with no mouse
 * involved. Search state (query, results, highlight) is local to this
 * component rather than in the billing store, so typing here never
 * re-renders the cart panel, and the cart's own updates never re-render this.
 */
export function ItemSearchBar({ currency }: ItemSearchBarProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Item[]>([]);
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const requestIdRef = useRef(0);

  const addItem = useBillingStore((state) => state.addItem);

  // Debounced so fast typing (or a barcode scanner, which types character by
  // character) doesn't fire a search per keystroke — only once typing pauses.
  const debouncedQuery = useDebouncedValue(query, 120);

  useEffect(() => {
    const trimmed = debouncedQuery.trim();
    if (!trimmed) {
      setResults([]);
      setError(null);
      return;
    }

    const requestId = ++requestIdRef.current;
    searchItems(trimmed)
      .then((items) => {
        // A slower, earlier request must never clobber a newer result set.
        if (requestId !== requestIdRef.current) return;
        setResults(items);
        setHighlightedIndex(0);
        setError(null);
      })
      .catch((e: Error) => {
        if (requestId !== requestIdRef.current) return;
        setError(e.message);
      });
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
    inputRef.current?.focus();
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      // A scanner's Enter arrives right after its digits; results may not
      // have caught up with the very latest keystroke yet, but the exact
      // barcode match is always sorted first by the backend, so acting on
      // whatever the current top result is remains correct.
      const top = results[highlightedIndex] ?? results[0];
      if (top) addAndReset(top);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      setHighlightedIndex((i) => Math.min(i + 1, results.length - 1));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setHighlightedIndex((i) => Math.max(i - 1, 0));
    } else if (event.key === "Escape") {
      setQuery("");
      setResults([]);
    }
  };

  return (
    <div className="relative">
      <div className="relative">
        <Search className="pointer-events-none absolute left-4 top-1/2 h-5 w-5 -translate-y-1/2 text-slate-400" />
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Search something sweet on your mind…"
          autoFocus
          className="w-full rounded-2xl bg-white py-3.5 pl-12 pr-4 text-sm shadow-soft placeholder:text-slate-400 focus:outline-none focus:ring-2 focus:ring-brand-200"
        />
      </div>

      {error && <p className="mt-1.5 text-sm text-red-600">{error}</p>}

      {results.length > 0 && (
        <ul className="absolute z-20 mt-1.5 max-h-80 w-full overflow-y-auto rounded-2xl bg-white shadow-soft-lg">
          {results.map((item, index) => (
            <li key={item.id}>
              <button
                type="button"
                onClick={() => addAndReset(item)}
                onMouseEnter={() => setHighlightedIndex(index)}
                disabled={item.stockQty <= 0}
                className={[
                  "flex w-full items-center justify-between gap-3 px-4 py-2.5 text-left text-sm disabled:cursor-not-allowed disabled:opacity-50",
                  index === highlightedIndex ? "bg-brand-50" : "hover:bg-slate-50",
                ].join(" ")}
              >
                <span className="min-w-0">
                  <span className="block truncate font-medium text-slate-900">{item.name}</span>
                  <span className="block text-xs text-slate-500">
                    {item.barcode ?? "No barcode"} · {item.stockQty} in stock
                  </span>
                </span>
                <span className="shrink-0 font-medium text-slate-900">
                  {formatMinor(item.priceMinor, currency)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
