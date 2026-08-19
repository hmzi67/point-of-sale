import type { CategoryTotal, Expense } from "../types";
import { call } from "./tauriClient";

/** Logs one expense. `amountMinor` is integer minor units. */
export function addExpense(
  date: string,
  category: string,
  amountMinor: number,
  note: string | null,
): Promise<Expense> {
  return call<Expense>("expenses_add_expense", { date, category, amountMinor, note });
}

/** Expenses in a date range (inclusive), optionally scoped to one category. */
export function getExpenses(
  startDate: string,
  endDate: string,
  category: string | null,
): Promise<Expense[]> {
  return call<Expense[]>("expenses_get_expenses", { startDate, endDate, category });
}

/** Every distinct category already in use, for the quick-add form's dropdown. */
export function getExpenseCategories(): Promise<string[]> {
  return call<string[]>("expenses_get_expense_categories", {});
}

/** Category-wise totals for a date range, highest spend first. */
export function getExpenseTotalsByCategory(startDate: string, endDate: string): Promise<CategoryTotal[]> {
  return call<CategoryTotal[]>("expenses_get_totals_by_category", { startDate, endDate });
}
