import { useEffect, useState } from "react";
import { Search, UserPlus } from "lucide-react";
import { searchCustomers } from "../../services/customersService";
import type { Customer } from "../../types";
import { formatMinor } from "../../utils/format";
import { CustomerFormModal } from "../customers/CustomerFormModal";
import { inputClass, ModalCard } from "../ui/ModalCard";

/** Finds a customer account by name, phone or email for the current sale —
 * or creates one on the spot. */
export function CustomerPickerModal({
  currency,
  onPick,
  onClose,
}: {
  currency: string;
  onPick: (customer: Customer) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Customer[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      searchCustomers(query)
        .then((list) => {
          setResults(list);
          setError(null);
        })
        .catch((e: Error) => setError(e.message));
    }, 150);
    return () => window.clearTimeout(handle);
  }, [query]);

  if (isCreating) {
    return <CustomerFormModal customer={null} initialName={query} onSaved={onPick} onClose={() => setIsCreating(false)} />;
  }

  return (
    <ModalCard title="Customer" subtitle="Attach a customer to this sale" width="sm" onClose={onClose}>
      <div className="relative">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-muted" />
        <input
          autoFocus
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Name, phone or email"
          className={`${inputClass} pl-9`}
        />
      </div>
      {error && <p className="mt-2 text-xs text-danger-text">{error}</p>}
      <ul className="mt-3 max-h-72 space-y-1 overflow-y-auto">
        {results.map((customer) => (
          <li key={customer.id}>
            <button
              type="button"
              onClick={() => onPick(customer)}
              className="flex w-full items-center justify-between gap-3 rounded-xl px-3 py-2.5 text-left hover:bg-surface-2"
            >
              <span className="min-w-0">
                <span className="block truncate text-sm font-medium text-fg">{customer.name}</span>
                <span className="block truncate text-xs text-fg-muted">
                  {[customer.phone, customer.email].filter(Boolean).join(" · ") || "No contact details"}
                </span>
              </span>
              {customer.balanceMinor > 0 && (
                <span className={`shrink-0 text-xs font-semibold ${customer.overdueMinor > 0 ? "text-danger-text" : "text-warning-text"}`}>
                  Owes {formatMinor(customer.balanceMinor, currency)}
                </span>
              )}
            </button>
          </li>
        ))}
        {results.length === 0 && !error && <li className="px-3 py-4 text-center text-xs text-fg-muted">No matching customers.</li>}
      </ul>
      <button
        type="button"
        onClick={() => setIsCreating(true)}
        className="mt-3 flex w-full items-center justify-center gap-2 rounded-xl border border-dashed border-line-strong py-2.5 text-sm font-medium text-fg-2 hover:bg-surface-2"
      >
        <UserPlus className="h-4 w-4" />
        {query.trim() ? `Add "${query.trim()}" as a new customer` : "Add a new customer"}
      </button>
    </ModalCard>
  );
}
