import { expect, test } from "vitest";
import { addPath, removePath, serial } from "./form";

test("paths are added once and removed by value", () => {
  expect(addPath(["/A"], "/B")).toEqual(["/A", "/B"]);
  expect(addPath(["/A"], "/A")).toEqual(["/A"]);
  expect(addPath(["/A"], null)).toEqual(["/A"]); // picker cancelled
  expect(removePath(["/A", "/B"], "/A")).toEqual(["/B"]);
});

test("serial runs saves in order, and a failed one does not block the next", async () => {
  const run = serial();
  const log: string[] = [];
  const wait = (ms: number, v: string, fail = false) => () =>
    new Promise<string>((ok, no) => setTimeout(() => (log.push(v), fail ? no(new Error(v)) : ok(v)), ms));
  const a = run(wait(30, "a"));
  const b = run(wait(0, "b", true));
  const c = run(wait(0, "c"));
  await expect(a).resolves.toBe("a");
  await expect(b).rejects.toThrow("b");
  await expect(c).resolves.toBe("c");
  expect(log).toEqual(["a", "b", "c"]);
});
