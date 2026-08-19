import { useState, type FormEvent } from "react";
import { Plus } from "lucide-react";
import { addExpense } from "../../services/expensesService";
import { decimalToMinor } from "../../utils/format";
import type { Expense } from "../../types";

function todayString(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

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

    setIsSaving(true);
    setError(null);
    try {
      const expense = await addExpense(date, resolvedCategory, amountMinor, note.trim() || null);
      onAdded(expense);
      setAmount("");
      setNote("");
      if (isAddingCategory) {
        setCategory(resolvedCategory);
        setIsAddingCategory(false);
        setNewCategory("");
      }
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <form
      onSubmit={(e) => void submit(e)}
      className="flex flex-wrap items-end gap-3 rounded-md border border-slate-200 bg-slate-50 p-3"
    >
      <div>
        <label className="block text-xs font-medium text-slate-500">Amount</label>
        <input
          type="number"
          min={0}
          step="0.01"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          placeholder="0.00"
          className="mt-1 w-28 rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
        />
      </div>

      <div>
        <label className="block text-xs font-medium text-slate-500">Category</label>
        {isAddingCategory ? (
          <div className="mt-1 flex items-center gap-1.5">
            <input
              value={newCategory}
              onChange={(e) => setNewCategory(e.target.value)}
              placeholder="New category"
              autoFocus
              className="w-36 rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
            />
            {categories.length > 0 && (
              <button
                type="button"
                onClick={() => {
                  setIsAddingCategory(false);
                  setNewCategory("");
                }}
                className="text-xs text-slate-500 hover:text-slate-700"
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
              className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
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
              className="flex items-center gap-1 text-xs font-medium text-brand-600 hover:text-brand-700"
            >
              <Plus className="h-3 w-3" />
              New
            </button>
          </div>
        )}
      </div>

      <div>
        <label className="block text-xs font-medium text-slate-500">Date</label>
        <input
          type="date"
          value={date}
          max={todayString()}
          onChange={(e) => setDate(e.target.value)}
          className="mt-1 rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
        />
      </div>

      <div className="min-w-[10rem] flex-1">
        <label className="block text-xs font-medium text-slate-500">Note (optional)</label>
        <input
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder="e.g. Electricity bill"
          className="mt-1 w-full rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
        />
      </div>

      {error && <p className="text-xs text-red-600">{error}</p>}

      <button
        type="submit"
        disabled={isSaving || !resolvedCategory || !amount}
        className="rounded-md bg-brand-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-brand-700 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {isSaving ? "Adding…" : "Add expense"}
      </button>
    </form>
  );
}
