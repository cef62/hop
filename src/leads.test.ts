import { expect, test } from "vitest";
import { calc } from "./calc";
import { insert, insertable, isLead, leads, rowAt, SHOWN, total, type Entry } from "./leads";
import { step, type Event, type State } from "./confirm";
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
  expect(leads("2 + 2", "4", history, true).filter((r) => r.kind === "history")).toEqual(hist(5).map(h));
});

test("an entry with stray spaces still counts as the query (a hand-edited file)", () => {
  const l = leads("2+2", "4", [{ expr: " 2+2 ", result: "4" }, ...hist(2)], true);
  expect(l.filter((r) => r.kind === "history")).toEqual(hist(2).map(h));
});

test("= alone lists up to twenty, no live row; spaces around it count as =", () => {
  for (const q of ["=", " = ", "  =", "=\t"]) {
    expect(leads(q, null, hist(25), true)).toEqual(hist(20).map(h));
    expect(leads(q, null, hist(25), true)).toHaveLength(20);
    expect(leads(q, null, hist(20), true)).toHaveLength(20);
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

const strip = (q: string) => q.replace(/\s+/g, "");
const old = { expr: "12*7+3", result: "87" };

/** What the palette does for a ⌘Enter on the first history row under `query`: leads, then the reducer. */
function cmdEnter(query: string, history: Entry[] = [old], result: string | null = calc(query)) {
  const lead = leads(query, result, history, true);
  const at = lead.findIndex((l) => l.kind === "history");
  if (at < 0) return { lead, query: undefined, noop: false };
  const ev: Event = { type: "enter", meta: true, repeat: false };
  const o = step({ armed: null, sel: at } as State, ev, (i) => lead[i], query);
  return { lead, query: o.query, noop: o.query === undefined && o.run === null };
}

test("an open sum (ends in an operator or an open parenthesis) shows the latest five history rows and no live row", () => {
  for (const q of ["1+", "12*", "1-", "2^", "(", "1+(", "(1+", " 1+ ", "6×", "9÷"]) {
    const l = leads(q, calc(q), hist(8), true);
    expect(l, q).toEqual(hist(5).map(h));
  }
  // not open: no leads at all (a bare number, a word, a closed or broken query)
  for (const q of ["safari", "1", "12", "1+)", "2**", "", "   ", ")", "(1)", "e+", "-)"]) expect(leads(q, calc(q), hist(8), true), q).toEqual([]);
  expect(leads("1+", null, hist(8), false)).toEqual([]);
});

test("⌘Enter on a history row: `1+` becomes `1+87` (88), `=` and an empty query become the result", () => {
  expect(cmdEnter("1+").query).toBe("1+87");
  expect(calc("1+87")).toBe("88");
  expect(cmdEnter("12*").query).toBe("12*87");
  expect(cmdEnter("(").query).toBe("(87");
  expect(cmdEnter("=").query).toBe("87");
  expect(cmdEnter(" = ").query).toBe("87");
  expect(cmdEnter("1+ ").query).toBe("1+ 87");
  // a negative result after a minus: `1--5` parses (6)
  const neg = [{ expr: "2-7", result: "-5" }];
  expect(cmdEnter("1-", neg).query).toBe("1--5");
  expect(calc("1--5")).toBe("6");
  // an exponent result after an operator still parses
  expect(calc(cmdEnter("1+", [{ expr: "2^64", result: "1.84467440737e+19" }]).query!)).toBe("1.84467440737e+19");
});

test("⌘Enter on a history row under a complete sum or a number does nothing", () => {
  for (const q of ["1+2", "2+2 ", "(1+2)", "3*4 "]) {
    const o = cmdEnter(q);
    expect(o.lead.some((l) => l.kind === "history"), q).toBe(true);
    expect(o.query, q).toBeUndefined();
    expect(o.noop, q).toBe(true);
  }
  expect([insertable("1+"), insertable("="), insertable(" "), insertable(""), insertable("("), insertable("1+2"), insertable("12"), insertable("(1+2)")]).toEqual([true, true, true, true, true, false, false, false]);
});

test("whitespace does not make another expression: leads drops the entry typed as the query", () => {
  const l = leads("2 + 2", "4", [{ expr: "2+2", result: "4" }, ...hist(6)], true);
  expect(l.filter((r) => r.kind === "history")).toEqual(hist(5).map(h));
  expect(strip("2 + 2")).toBe("2+2");
});


test("insert: the result alone for = or nothing, appended after an open sum, otherwise unchanged", () => {
  const exp = "1.84467440737e+19";
  expect(insert("1+", "5")).toBe("1+5");
  expect(insert("", "7")).toBe("7");
  expect(insert("=", "7")).toBe("7");
  expect(insert("12", "5")).toBe("12");
  expect(insert("1+2", "5")).toBe("1+2");
  expect(calc(insert("1+", exp))).toBe(exp);
  expect(calc(insert("2*", "1e-7"))).toBe("2e-7");
  expect(calc(insert("(1+", "5") + ")")).toBe("6");
  expect(insert("12+3*", "5")).toBe("12+3*5");
});
