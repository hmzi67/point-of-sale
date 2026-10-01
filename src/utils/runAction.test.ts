import { describe, expect, it } from "vitest";
import { runAction } from "./runAction";

function recorder() {
  const busy: boolean[] = [];
  const errors: Array<string | null> = [];
  return { busy, errors, setBusy: (b: boolean) => busy.push(b), setError: (e: string | null) => errors.push(e) };
}

describe("runAction", () => {
  it("toggles busy around a successful action and clears the error", async () => {
    const r = recorder();
    expect(await runAction(r.setBusy, r.setError, async () => undefined)).toBe(true);
    expect(r.busy).toEqual([true, false]);
    expect(r.errors).toEqual([null]);
  });

  it("reports a failure's message, prefixed when asked, and still clears busy", async () => {
    const r = recorder();
    const fail = async () => {
      throw new Error("printer offline");
    };
    expect(await runAction(r.setBusy, r.setError, fail, "Print failed")).toBe(false);
    expect(r.busy).toEqual([true, false]);
    expect(r.errors).toEqual([null, "Print failed: printer offline"]);

    const plain = recorder();
    await runAction(plain.setBusy, plain.setError, fail);
    expect(plain.errors[plain.errors.length - 1]).toBe("printer offline");
  });
});
