/** Expense tracking — see `src-tauri/src/db/expenses.rs`. */

export interface Expense {
  id: number;
  /** `YYYY-MM-DD`. */
  expenseDate: string;
  category: string;
  amountMinor: number;
  note: string | null;
}

export interface CategoryTotal {
  category: string;
  totalMinor: number;
  count: number;
}
