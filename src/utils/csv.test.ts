import { describe, expect, it } from "vitest";
import { buildCsv, toCsvRow } from "./csv";

describe("toCsvRow", () => {
  it("leaves plain fields and numbers alone", () => {
    expect(toCsvRow(["Cola", 3, 80.5])).toBe("Cola,3,80.5");
  });

  it("quotes fields containing a comma, newline or carriage return", () => {
    expect(toCsvRow(["a,b"])).toBe('"a,b"');
    expect(toCsvRow(["line1\nline2"])).toBe('"line1\nline2"');
    expect(toCsvRow(["stray\rcr"])).toBe('"stray\rcr"');
  });

  it("doubles embedded quotes", () => {
    expect(toCsvRow(['12" pizza'])).toBe('"12"" pizza"');
  });

  it("keeps empty fields as empty", () => {
    expect(toCsvRow(["", "x", ""])).toBe(",x,");
  });
});

describe("buildCsv", () => {
  it("joins rows with CRLF", () => {
    expect(buildCsv([["a", "b"], [1, 2]])).toBe("a,b\r\n1,2");
  });

  it("returns an empty string for no rows", () => {
    expect(buildCsv([])).toBe("");
  });
});
