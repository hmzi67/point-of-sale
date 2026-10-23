import { Receipt, Wallet, Layers } from "lucide-react";
import { ManagementSummary } from "../components/ui/ManagementSummary";
import { formatMinor } from "../utils/format";
import { useState } from "react";
import { AddExpenseForm } from "../components/expenses/AddExpenseForm";
import { CategoryBreakdown } from "../components/expenses/CategoryBreakdown";
import { ExpenseList } from "../components/expenses/ExpenseList";
import { useAppConfig } from "../hooks/useAppConfig";
import { useReport } from "../hooks/useReport";
import {
  getExpenseCategories,
  getExpenseTotalsByCategory,
  getExpenses,
} from "../services/expensesService";
import { PRESET_LABELS, resolveDateRangePreset } from "../utils/dateRanges";
import type { CategoryTotal, DateRangePreset, Expense } from "../types";
import { DateRangeInputs } from "../components/ui/DateRangeInputs";

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

  const { data, isLoading, error, reload } = useReport(
    () =>
      Promise.all([
        getExpenseCategories(),
        getExpenses(startDate, endDate, categoryFilter || null),
        getExpenseTotalsByCategory(startDate, endDate),
      ]),
    [startDate, endDate, categoryFilter],
    { enabled: Boolean(startDate && endDate && startDate <= endDate) },
  );
  const [categories, expenses, totals]: [string[], Expense[], CategoryTotal[]] = data ?? [[], [], []];

  const applyPreset = (p: Exclude<DateRangePreset, "custom">) => {
    const range = resolveDateRangePreset(p);
    setPreset(p);
    setStartDate(range.startDate);
    setEndDate(range.endDate);
  };

  return (
    <section className="space-y-6">
      <div>
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-accent-text">Business spending</p>
          <h2 className="text-2xl font-bold tracking-tight text-fg sm:text-3xl">Expenses</h2>
        <p className="mt-2 max-w-2xl text-sm leading-relaxed text-fg-muted">Log daily shop expenses and see where the money's going.</p>
      </div>

      <ManagementSummary isLoading={isLoading} items={[
        { label: "Expenses in view", value: formatMinor(expenses.reduce((sum, expense) => sum + expense.amountMinor, 0), config.currency), detail: "Total matching your dates and category", icon: Wallet },
        { label: "Expense entries", value: expenses.length, detail: "Records matching your current filters", icon: Receipt },
        { label: "Spending categories", value: totals.length, detail: "Across all categories in this date range", icon: Layers, tone: "warning" },
      ]} />

      <AddExpenseForm categories={categories} onAdded={reload} />

      <div className="flex flex-wrap items-center gap-3 rounded-2xl border border-line bg-surface p-4">
        <div className="flex flex-wrap gap-1 rounded-2xl border border-line bg-surface p-1.5">
          {PRESETS.map((p) => (
            <button
              key={p}
              type="button"
              onClick={() => applyPreset(p)}
              aria-pressed={preset === p}
              className={[
                "rounded-lg px-4 py-2.5 text-sm font-medium transition-colors",
                preset === p ? "bg-accent text-accent-ink" : "text-fg-2 hover:bg-surface-3",
              ].join(" ")}
            >
              {PRESET_LABELS[p]}
            </button>
          ))}
        </div>

        <DateRangeInputs
          label="Expense"
          start={startDate}
          end={endDate}
          onStartChange={(value) => {
            setPreset("custom");
            setStartDate(value);
          }}
          onEndChange={(value) => {
            setPreset("custom");
            setEndDate(value);
          }}
        />

        <select
          aria-label="Expense category"
          value={categoryFilter}
          onChange={(e) => setCategoryFilter(e.target.value)}
          className="rounded-xl border border-line-strong px-3 py-2.5 text-sm"
        >
          <option value="">All categories</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </div>

      {error && <p className="rounded-xl bg-danger-soft px-3 py-2 text-sm text-danger-text">{error}</p>}

      <div className="grid grid-cols-1 items-start gap-5 lg:grid-cols-3">
        <div className="lg:col-span-2">
          <ExpenseList expenses={expenses} currency={config.currency} isLoading={isLoading} />
        </div>
        <CategoryBreakdown totals={totals} currency={config.currency} isLoading={isLoading} />
      </div>
    </section>
  );
}
