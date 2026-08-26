/** Physical kitchen/preparation stations for the KOT token workflow — see
 * `src-tauri/src/db/counters.rs`. Deliberately separate from `Category`
 * (billing browse/filter grouping); a client may serve one category from
 * multiple counters or vice versa. */

export interface Counter {
  id: number;
  name: string;
  isActive: boolean;
}
