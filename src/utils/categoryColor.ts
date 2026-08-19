/**
 * A deterministic pastel color per category, for the small tag pills on item
 * cards/pills. Categories are entirely client-defined (Inventory → Add
 * category), so there is no fixed name→color map to hardcode — instead every
 * category id is hashed into one slot of a fixed palette. The same category
 * always lands on the same color within one session (and across sessions,
 * since the hash is a pure function of the id), without needing a `color`
 * column on `categories` just for this.
 */

const PALETTE: { bg: string; text: string; dot: string }[] = [
  { bg: "bg-orange-50", text: "text-orange-600", dot: "bg-orange-400" },
  { bg: "bg-emerald-50", text: "text-emerald-600", dot: "bg-emerald-400" },
  { bg: "bg-sky-50", text: "text-sky-600", dot: "bg-sky-400" },
  { bg: "bg-pink-50", text: "text-pink-600", dot: "bg-pink-400" },
  { bg: "bg-violet-50", text: "text-violet-600", dot: "bg-violet-400" },
  { bg: "bg-amber-50", text: "text-amber-600", dot: "bg-amber-400" },
  { bg: "bg-teal-50", text: "text-teal-600", dot: "bg-teal-400" },
  { bg: "bg-rose-50", text: "text-rose-600", dot: "bg-rose-400" },
];

/** `null`/`undefined` (no category) gets a neutral slate tag rather than a
 * palette slot, so "uncategorized" reads as deliberately different, not as
 * category #0. */
const NEUTRAL = { bg: "bg-slate-100", text: "text-slate-500", dot: "bg-slate-400" };

export function categoryColor(categoryId: number | null | undefined) {
  if (categoryId === null || categoryId === undefined) return NEUTRAL;
  return PALETTE[categoryId % PALETTE.length];
}
