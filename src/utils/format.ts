/** Currency/date helpers shared across modules. Kept dependency-free and offline-safe. */

export function formatCurrency(amount: number, currency = "PKR"): string {
  return `${currency} ${amount.toFixed(2)}`;
}

/** Integer minor units (paisa) -> a decimal amount, for display or form inputs. */
export function minorToDecimal(minor: number): number {
  return minor / 100;
}

/** A decimal amount (e.g. from a form input) -> integer minor units. Rounds
 * to the nearest cent so floating-point input error never reaches storage. */
export function decimalToMinor(amount: number): number {
  return Math.round(amount * 100);
}

export function formatMinor(minor: number, currency = "PKR"): string {
  return formatCurrency(minorToDecimal(minor), currency);
}

export function formatDate(date: Date | string): string {
  const d = typeof date === "string" ? new Date(date) : date;
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** Parses a `YYYY-MM-DD` string as a local-time date, not UTC midnight —
 * `new Date("YYYY-MM-DD")` parses as UTC, which shifts a day in negative-UTC
 * timezones once formatted back with the browser's local zone. */
export function parseLocalDateString(dateStr: string): Date {
  const [year, month, day] = dateStr.split("-").map(Number);
  return new Date(year, month - 1, day);
}

/** A `YYYY-MM-DD` string -> "Aug 5", for compact chart axis labels. */
export function formatShortDate(dateStr: string): string {
  return parseLocalDateString(dateStr).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** A SQLite `'YYYY-MM-DD HH:MM:SS'` timestamp -> "9:05 AM", for attendance
 * check-in/out times. Parsed as local time (space instead of `T` keeps the
 * browser from reading it as UTC), not through `formatDate`, which only
 * carries the date. */
export function formatTime(timestamp: string): string {
  const [datePart, timePart] = timestamp.split(" ");
  if (!datePart || !timePart) return timestamp;
  const [year, month, day] = datePart.split("-").map(Number);
  const [hour, minute, second] = timePart.split(":").map(Number);
  const d = new Date(year, month - 1, day, hour, minute, second);
  return d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

/** Decimal hours (e.g. `8.5`) -> "8h 30m". */
export function formatHours(hours: number): string {
  const totalMinutes = Math.round(hours * 60);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}
