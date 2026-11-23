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

/** The lead rows for `query`. Off (the Calculator switch): none, `=` is a plain search. Exactly `=` (spaces around it
 * are ignored): the whole history, no live row. A sum (`result` not null): its answer, then the latest `SHOWN`
 * entries, leaving out the one that is the query itself (it would only repeat the live row). Anything else: none. */
export function leads(query: string, result: string | null, history: Entry[], on: boolean): Lead[] {
  const q = query.trim();
  const past = (list: Entry[], n: number): Lead[] => list.slice(0, n).map((e) => ({ kind: "history", expr: e.expr, result: e.result }));
  if (!on) return [];
  if (q === "=") return past(history, ALL);
  if (result === null) return [];
  return [{ kind: "result", text: result }, ...past(history.filter((e) => e.expr.trim() !== q), SHOWN)];
}

/** What row `i` of the list is: a lead, or an item after them. */
export const rowAt = (i: number, lead: Lead[], rows: Item[]): Row => (i < lead.length ? lead[i] : rows[i - lead.length]);
export const total = (rows: Item[], lead: Lead[]) => rows.length + lead.length;

/** The query after ⌘Enter on a history row: its result appended as it is. Typed after an operator ("1+") it
 * completes the sum; after a number ("12") the digits run together ("125"), and that is the rule, with no guessing which
 * the user meant. Every result calc shows parses when typed back, exponent form included. */
export const insert = (query: string, result: string) => query + result;
