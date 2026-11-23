import { expect, test } from "vitest";
import { calc } from "./calc";
import { ALL, insert, isLead, leads, rowAt, SHOWN, total, type Entry } from "./leads";
import type { Item } from "./rank";

const hist = (n: number): Entry[] => Array.from({ length: n }, (_, i) => ({ expr: `${i}+1`, result: String(i + 1) }));
const item = (name: string): Item => ({ kind: "app", id: `/A/${name}.app`, name, altNames: [], path: `/A/${name}.app` });
const h = (e: Entry) => ({ kind: "history", ...e });

test("a sum: its answer, then the latest five entries", () => {
  const l = leads("2+2", "4", hist(25), true);
  expect(l).toHaveLength(1 + SHOWN);
  expect(l[0]).toEqual({ kind: "result", text: "4" });
  expect(l.slice(1)).toEqual(hist(5).map(h));
  expect(leads("2+2", "4", hist(0), true)).toEqual([{ kind: "result", text: "4" }]);
  expect(leads("2+2", "4", hist(1), true)).toHaveLength(2);
});

test("a history entry equal to the query is not repeated, and the next ones fill its place", () => {
  const history = [{ expr: "2+2", result: "4" }, ...hist(6)];
  const l = leads("2+2", "4", history, true);
  expect(l.filter((r) => r.kind === "history")).toEqual(hist(5).map(h));
  // spaces do not make it another query
  expect(leads(" 2+2 ", "4", history, true).filter((r) => r.kind === "history")).toEqual(hist(5).map(h));
  expect(leads("2 + 2", "4", history, true)[1]).toEqual(h({ expr: "2+2", result: "4" }));
});

test("= alone lists up to twenty, no live row; spaces around it count as =", () => {
  for (const q of ["=", " = ", "  =", "=\t"]) {
    expect(leads(q, null, hist(25), true)).toEqual(hist(ALL).map(h));
    expect(leads(q, null, hist(1), true)).toEqual(hist(1).map(h));
    expect(leads(q, null, hist(0), true)).toEqual([]);
  }
  // not exactly =
  expect(leads("==", null, hist(3), true)).toEqual([]);
  expect(leads("=1", null, hist(3), true)).toEqual([]);
});

test("the Calculator switch off: no leads at all, = is a plain search", () => {
  expect(leads("2+2", "4", hist(3), false)).toEqual([]);
  expect(leads("=", null, hist(3), false)).toEqual([]);
});

test("a query that is not a sum has no leads", () => {
  expect(leads("safari", null, hist(3), true)).toEqual([]);
  expect(leads("", null, hist(3), true)).toEqual([]);
});

test("rowAt and total count the leads before the items", () => {
  const l = leads("2+2", "4", hist(5), true);
  const rows = ["a", "b", "c"].map(item);
  expect(total(rows, l)).toBe(9);
  expect(rowAt(0, l, rows)).toBe(l[0]);
  expect(rowAt(5, l, rows)).toBe(l[5]);
  expect(rowAt(6, l, rows)).toBe(rows[0]);
  expect(rowAt(8, l, rows)).toBe(rows[2]);
  expect(rowAt(9, l, rows)).toBeUndefined();
  expect(rowAt(0, [], rows)).toBe(rows[0]);
  expect(total(rows, [])).toBe(3);
  expect(isLead(l[0]) && isLead(l[1]) && !isLead(rows[0]) && !isLead(undefined)).toBe(true);
});

test("insert appends the result as it is: after an operator it completes the sum, after a number the digits run on", () => {
  const exp = "1.84467440737e+19";
  expect(insert("1+", "5")).toBe("1+5");
  expect(calc(insert("1+", exp))).toBe(exp); // Review Focus 4: still parses
  expect(calc(insert("1+", "-5"))).toBe("-4");
  expect(calc(insert("2*", "1e-7"))).toBe("2e-7");
  expect(calc(insert("(1+", "5") + ")")).toBe("6");
  // after a digit: "12" and "5" are "125", no sum yet; documented, not guessed
  expect(insert("12", "5")).toBe("125");
  expect(calc(insert("12", "5"))).toBeNull();
  expect(calc(insert("12+3*", "5"))).toBe("27");
  // a number then an exponent result is one longer number, still a valid literal
  expect(insert("12", exp)).toBe("121.84467440737e+19");
  expect(calc(insert("12", exp) + "*1")).not.toBeNull();
  expect(insert("", "7")).toBe("7");
  expect(insert("1+ ", "5")).toBe("1+ 5");
});
