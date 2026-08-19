import { useEffect, useState } from "react";
import { AddExpenseForm } from "../components/expenses/AddExpenseForm";
import { CategoryBreakdown } from "../components/expenses/CategoryBreakdown";
import { ExpenseList } from "../components/expenses/ExpenseList";
import { useAppConfig } from "../hooks/useAppConfig";
import {
  getExpenseCategories,
  getExpenseTotalsByCategory,
  getExpenses,
} from "../services/expensesService";
import { PRESET_LABELS, resolveDateRangePreset } from "../utils/dateRanges";
import type { CategoryTotal, DateRangePreset, Expense } from "../types";

const PRESETS: Exclude<DateRangePreset, "custom">[] = ["today", "thisWeek", "thisMonth"];

/** One shared date range (+ optional category filter) drives both the log
 * and the category breakdown, so the two views never disagree about what
 * period they're summarizing — the same range Phase 10's dashboard will ask
 * for when it pulls this module's totals into the profit calculation. */
export function ExpensesPage() {
  const { config } = useAppConfig();

  const initialRange = resolveDateRangePreset("thisMonth");
  const [preset, setPreset] = useState<DateRangePreset>("thisMonth");
  const [startDate, setStartDate] = useState(initialRange.startDate);
  const [endDate, setEndDate] = useState(initialRange.endDate);
  const [categoryFilter, setCategoryFilter] = useState<string>("");

  const [categories, setCategories] = useState<string[]>([]);
  const [expenses, setExpenses] = useState<Expense[]>([]);
  const [totals, setTotals] = useState<CategoryTotal[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = () => {
    if (!startDate || !endDate || startDate > endDate) return;
    setIsLoading(true);
    setError(null);
    Promise.all([
      getExpenseCategories(),
      getExpenses(startDate, endDate, categoryFilter || null),
      getExpenseTotalsByCategory(startDate, endDate),
    ])
      .then(([cats, list, breakdown]) => {
        setCategories(cats);
        setExpenses(list);
        setTotals(breakdown);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setIsLoading(false));
  };

  useEffect(reload, [startDate, endDate, categoryFilter]);

  const applyPreset = (p: Exclude<DateRangePreset, "custom">) => {
    const range = resolveDateRangePreset(p);
    setPreset(p);
    setStartDate(range.startDate);
    setEndDate(range.endDate);
  };

  return (
    <section className="space-y-4">
      <div>
        <h2 className="text-lg font-semibold text-slate-900">Expenses</h2>
        <p className="text-sm text-slate-500">Log daily shop expenses and see where the money's going.</p>
      </div>

      <AddExpenseForm categories={categories} onAdded={reload} />

      <div className="flex flex-wrap items-center gap-2">
        <div className="flex rounded-md border border-slate-300 p-0.5">
          {PRESETS.map((p) => (
            <button
              key={p}
              type="button"
              onClick={() => applyPreset(p)}
              className={[
                "rounded px-3 py-1.5 text-sm font-medium transition-colors",
                preset === p ? "bg-brand-600 text-white" : "text-slate-600 hover:bg-slate-100",
              ].join(" ")}
            >
              {PRESET_LABELS[p]}
            </button>
          ))}
        </div>

        <div className="flex items-center gap-1.5">
          <input
            type="date"
            value={startDate}
            max={endDate || undefined}
            onChange={(e) => {
              setPreset("custom");
              setStartDate(e.target.value);
            }}
            className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
          />
          <span className="text-slate-400">–</span>
          <input
            type="date"
            value={endDate}
            min={startDate || undefined}
            onChange={(e) => {
              setPreset("custom");
              setEndDate(e.target.value);
            }}
            className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
          />
        </div>

        <select
          value={categoryFilter}
          onChange={(e) => setCategoryFilter(e.target.value)}
          className="rounded-md border border-slate-300 px-2.5 py-1.5 text-sm"
        >
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </div>

      {error && <p className="rounded-md bg-red-50 px-3 py-2 text-sm text-red-700">{error}</p>}

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <div className="lg:col-span-2">
          <ExpenseList expenses={expenses} currency={config.currency} isLoading={isLoading} />
        </div>
        <CategoryBreakdown totals={totals} currency={config.currency} isLoading={isLoading} />
      </div>
    </section>
  );
}
