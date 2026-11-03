import { describe, expect, it } from "vitest";
import type { TableSummary } from "../types";
import { groupByFloor } from "./tableFloors";

const table = (id: number, floor: string | null) => ({ id, floor }) as TableSummary;

describe("groupByFloor", () => {
  it("sorts floors and puts tables without one last", () => {
    const groups = groupByFloor([table(1, null), table(2, "Out Side"), table(3, "Main hall"), table(4, "Out Side")]);
    expect(groups.map((g) => [g.floor, g.tables.map((t) => t.id)])).toEqual([
      ["Main hall", [3]],
      ["Out Side", [2, 4]],
      [null, [1]],
    ]);
  });

  it("gives a shop with no floors one group", () => {
    expect(groupByFloor([table(1, null)]).map((g) => g.floor)).toEqual([null]);
  });
});
