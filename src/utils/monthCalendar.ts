/** One month for a Monday-first calendar: how many blank cells lead the first
 * row, and every date as `YYYY-MM-DD`. `monthIndex` is 0-based, like `Date`. */
export function monthGrid(year: number, monthIndex: number): { blanks: number; dates: string[] } {
  const prefix = `${year}-${String(monthIndex + 1).padStart(2, "0")}`;
  const days = new Date(year, monthIndex + 1, 0).getDate();
  return {
    blanks: (new Date(year, monthIndex, 1).getDay() + 6) % 7,
    dates: Array.from({ length: days }, (_, i) => `${prefix}-${String(i + 1).padStart(2, "0")}`),
  };
}
