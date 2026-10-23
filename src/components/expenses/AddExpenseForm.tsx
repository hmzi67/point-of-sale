import { useState, type FormEvent } from "react";
import { Plus } from "lucide-react";
import { addExpense } from "../../services/expensesService";
import { decimalToMinor } from "../../utils/format";
import type { Expense } from "../../types";
import { todayString } from "../../utils/dateRanges";
import { runAction } from "../../utils/runAction";
import { inputClass } from "../ui/ModalCard";

interface AddExpenseFormProps {
  categories: string[];
  onAdded: (expense: Expense) => void;
}

/** Quick-add expense entry: amount, category (existing ones in a dropdown,
 * plus an inline "add new category" that just switches to a free-text
 * input — there's no categories table to insert into first, see
 * `db/expenses.rs`), date (defaults to today), and an optional note. */
export function AddExpenseForm({ categories, onAdded }: AddExpenseFormProps) {
  const [amount, setAmount] = useState("");
  const [category, setCategory] = useState(categories[0] ?? "");
  const [isAddingCategory, setIsAddingCategory] = useState(categories.length === 0);
  const [newCategory, setNewCategory] = useState("");
  const [date, setDate] = useState(todayString());
  const [note, setNote] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const resolvedCategory = isAddingCategory ? newCategory.trim() : category;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const amountMinor = decimalToMinor(Number(amount));
    if (!resolvedCategory || !amountMinor || amountMinor <= 0 || !date) return;

    await runAction(setIsSaving, setError, async () => {
      const expense = await addExpense(date, resolvedCategory, amountMinor, note.trim() || null);
      onAdded(expense);
      setAmount("");
      setNote("");
      if (isAddingCategory) {
        setCategory(resolvedCategory);
        setIsAddingCategory(false);
        setNewCategory("");
      }
    });
  };

  return (
    <form
      onSubmit={(e) => void submit(e)}
      className="flex flex-wrap items-end gap-4 rounded-2xl border border-line bg-surface p-5"
    >
      <div className="w-full border-b border-line pb-4">
        <h3 className="text-sm font-semibold text-fg">Record an expense</h3>
        <p className="mt-1 text-xs text-fg-muted">Keep everyday costs organized in one place.</p>
      </div>
      <div>
        <label className="block text-xs font-medium text-fg-muted">Amount</label>
        <input
          type="number"
          min={0}
          step="0.01"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          placeholder="0.00"
          className="mt-1 w-28 rounded-xl border border-line-strong px-3 py-2.5 text-sm"
        />
      </div>

      <div>
        <label className="block text-xs font-medium text-fg-muted">Category</label>
        {isAddingCategory ? (
          <div className="mt-1 flex items-center gap-1.5">
            <input
              value={newCategory}
              onChange={(e) => setNewCategory(e.target.value)}
              placeholder="New category"
              autoFocus
              className="w-36 rounded-xl border border-line-strong px-3 py-2.5 text-sm"
            />
            {categories.length > 0 && (
              <button
                type="button"
                onClick={() => {
                  setIsAddingCategory(false);
                  setNewCategory("");
                }}
                className="text-xs text-fg-muted hover:text-fg-2"
              >
                Cancel
              </button>
            )}
          </div>
        ) : (
          <div className="mt-1 flex items-center gap-1.5">
            <select
              value={category}
              onChange={(e) => setCategory(e.target.value)}
              className="rounded-xl border border-line-strong px-3 py-2.5 text-sm"
            >
              {categories.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={() => setIsAddingCategory(true)}
              className="flex items-center gap-1 text-xs font-medium text-accent-text hover:text-accent-text"
            >
              <Plus className="h-3 w-3" />
              New
            </button>
          </div>
        )}
      </div>

      <div>
        <label className="block text-xs font-medium text-fg-muted">Date</label>
        <input
          type="date"
          value={date}
          max={todayString()}
          onChange={(e) => setDate(e.target.value)}
          className="mt-1 rounded-xl border border-line-strong px-3 py-2.5 text-sm"
        />
      </div>

      <div className="min-w-[10rem] flex-1">
        <label className="block text-xs font-medium text-fg-muted">Note (optional)</label>
        <input
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder="e.g. Electricity bill"
          className={`mt-1 ${inputClass}`}
        />
      </div>

      {error && <p className="text-xs text-danger-text">{error}</p>}

      <button
        type="submit"
        disabled={isSaving || !resolvedCategory || !amount}
        className="rounded-xl bg-accent px-3 py-1.5 text-sm font-medium text-accent-ink hover:bg-accent-hover disabled:cursor-not-allowed disabled:opacity-50"
      >
        {isSaving ? "Adding…" : "Add expense"}
      </button>
    </form>
  );
}
