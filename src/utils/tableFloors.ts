import type { TableSummary } from "../types";

/** Tables grouped by floor, floors alphabetically, tables with no floor last
 * (a shop that uses no floors gets one group with a `null` floor). */
export function groupByFloor(tables: TableSummary[]): { floor: string | null; tables: TableSummary[] }[] {
  const byFloor = new Map<string | null, TableSummary[]>();
  for (const table of tables) {
    const group = byFloor.get(table.floor ?? null);
    if (group) group.push(table);
    else byFloor.set(table.floor ?? null, [table]);
  }
  return [...byFloor.entries()]
    .sort(([a], [b]) => (a === null ? 1 : b === null ? -1 : a.localeCompare(b)))
    .map(([floor, grouped]) => ({ floor, tables: grouped }));
}
