/**
 * A deterministic pastel color per category, for the small tag pills on item
 * cards/pills. Categories are entirely client-defined (Inventory → Add
 * category), so there is no fixed name→color map to hardcode — instead every
 * category id is hashed into one slot of a fixed palette. The same category
 * always lands on the same color within one session (and across sessions,
 * since the hash is a pure function of the id), without needing a `color`
 * column on `categories` just for this.
 */

/* These are hue-categorical (a category must stay "the pink one" in both
 * themes), so they're the one place that uses raw palette hues with a
 * `dark:` text variant instead of the semantic tokens — the translucent
 * `/15` fill tints whatever ground it sits on, so only the text needs the
 * lighter shade on dark. */
const PALETTE: { bg: string; text: string; dot: string }[] = [
  { bg: "bg-amber-500/15", text: "text-amber-700 dark:text-amber-300", dot: "bg-amber-400" },
  { bg: "bg-emerald-500/15", text: "text-emerald-700 dark:text-emerald-300", dot: "bg-emerald-400" },
  { bg: "bg-sky-500/15", text: "text-sky-700 dark:text-sky-300", dot: "bg-sky-400" },
  { bg: "bg-pink-500/15", text: "text-pink-700 dark:text-pink-300", dot: "bg-pink-400" },
  { bg: "bg-violet-500/15", text: "text-violet-700 dark:text-violet-300", dot: "bg-violet-400" },
  { bg: "bg-orange-500/15", text: "text-orange-700 dark:text-orange-300", dot: "bg-orange-400" },
  { bg: "bg-teal-500/15", text: "text-teal-700 dark:text-teal-300", dot: "bg-teal-400" },
  { bg: "bg-rose-500/15", text: "text-rose-700 dark:text-rose-300", dot: "bg-rose-400" },
];

/** `null`/`undefined` (no category) gets a neutral slate tag rather than a
 * palette slot, so "uncategorized" reads as deliberately different, not as
 * category #0. */
const NEUTRAL = { bg: "bg-surface-3", text: "text-fg-muted", dot: "bg-fg-faint" };

export function categoryColor(categoryId: number | null | undefined) {
  if (categoryId === null || categoryId === undefined) return NEUTRAL;
  return PALETTE[categoryId % PALETTE.length];
}
