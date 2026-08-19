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
