import { calc } from "./calc";
import type { Item } from "./rank";

/** One copied result, as `calc_history` sends it (`calc_history::Entry`). */
export type Entry = { expr: string; result: string };

/** The rows ahead of the items: the calculator's live answer, or a past result. */
export type Lead = { kind: "result"; text: string } | { kind: "history"; expr: string; result: string };
/** A palette row: a lead, an item, or nothing (past the end). */
export type Row = Item | Lead | undefined;

/** History rows under a live result, and for the query `=`. */
export const SHOWN = 5;
export const ALL = 20;

export const isLead = (r: Row): r is Lead => r?.kind === "result" || r?.kind === "history";

const bare = (q: string) => q.replace(/\s+/g, "");

/** A sum that waits for its last number: it ends in an operator or an open parenthesis (`1+`, `12*`, `(`), and a number
 * would complete it. Probed by calc itself (the parentheses closed, a term added) so the grammar lives in one place. */
export function open(query: string): boolean {
  const q = query.trim();
  if (!/[-+*/%^×÷(]$/.test(q)) return false;
  const opened = q.split("(").length - q.split(")").length;
  return opened >= 0 && calc(`${q}1${")".repeat(opened)}+1`) !== null;
}

/** The lead rows for `query`. Off (the Calculator switch): none, `=` is a plain search. Exactly `=` (spaces around it
 * are ignored): the whole history, no live row. A sum (`result` not null): its answer, then the latest `SHOWN`
 * entries, leaving out the one that is the query itself (spaces ignored: it would only repeat the live row). An open
 * sum (`1+`): the latest `SHOWN` entries alone, to pick a result to complete it with, unless an item matches the query (`matched`: `7-` is the start of
 * 7-Zip, and Enter must open it). Anything else: none. */
export function leads(query: string, result: string | null, history: Entry[], on: boolean, matched = false): Lead[] {
  const q = query.trim();
  const past = (list: Entry[], n: number): Lead[] => list.slice(0, n).map((e) => ({ kind: "history", expr: e.expr, result: e.result }));
  if (!on) return [];
  if (q === "=") return past(history, ALL);
  if (result === null) return open(q) && !matched ? past(history, SHOWN) : [];
  return [{ kind: "result", text: result }, ...past(history.filter((e) => bare(e.expr) !== bare(q)), SHOWN)];
}

/** What row `i` of the list is: a lead, or an item after them. */
export const rowAt = (i: number, lead: Lead[], rows: Item[]): Row => (i < lead.length ? lead[i] : rows[i - lead.length]);
export const total = (rows: Item[], lead: Lead[]) => rows.length + lead.length;

/** Where ⌘Enter on a history row can put its result: `=` or nothing (the result replaces it), or an open sum (`1+`, the
 * result completes it). After a complete sum or a number there is no place, and ⌘Enter does nothing. */
export const insertable = (query: string) => query.trim() === "" || query.trim() === "=" || open(query);

/** The query after ⌘Enter on a history row: the result alone for `=` or nothing, appended to an open sum as it is (`1+`
 * and `87` make `1+87`; exponent form and a negative result parse too: `1-` and `-5` make `1--5`), otherwise the query
 * as it was. */
export const insert = (query: string, result: string) => (!insertable(query) ? query : query.trim() === "" || query.trim() === "=" ? result : query + result);
