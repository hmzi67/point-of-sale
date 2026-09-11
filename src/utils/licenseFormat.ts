/** Display helpers for licence timestamps (unix seconds from Rust). Kept
 * out of `format.ts` because everything there takes ISO strings/Dates from
 * SQLite; these take the integer seconds `license::state` emits. */

export function formatUnixDateTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

/** "in 23 hours" / "in 45 minutes" / "in 3 days" / "now" — coarse on
 * purpose: the banner re-renders on the per-minute state event, not a timer,
 * so a seconds-precision countdown would look stuck. */
export function formatTimeUntil(seconds: number, nowSeconds = Date.now() / 1000): string {
  const delta = Math.max(0, Math.round(seconds - nowSeconds));
  if (delta < 60) return "now";
  const minutes = Math.round(delta / 60);
  if (minutes < 60) return `in ${minutes} minute${minutes === 1 ? "" : "s"}`;
  const hours = Math.round(delta / 3600);
  if (hours < 48) return `in ${hours} hour${hours === 1 ? "" : "s"}`;
  const days = Math.round(delta / 86400);
  return `in ${days} day${days === 1 ? "" : "s"}`;
}
