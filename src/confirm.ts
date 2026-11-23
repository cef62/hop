import { insert, isLead, type Row } from "./leads";
import type { Item } from "./rank";

/**
 * Enter (or ⌘N, or a click) on `item` while the command `armed` waits for its second Enter. An item without a
 * `confirm` prompt runs. One with a prompt runs only on a `plain` press (a plain Enter, not ⌘Enter or a held Enter's
 * repeat) of the armed row; a plain press elsewhere arms it, anything else disarms.
 */
export function press(armed: string | null, item: Item, plain: boolean): { armed: string | null; run: boolean } {
  if (!item.confirm) return { armed: null, run: true };
  if (plain && armed === item.id) return { armed: null, run: true };
  return { armed: plain ? item.id : null, run: false };
}

/** The selected row and the command waiting for its second Enter, with the query it was armed under. */
export type State = { armed: { id: string; q: string } | null; sel: number };
export type Event =
  | { type: "enter"; meta: boolean; repeat: boolean }
  /** ⌘N, or a click on a row */
  | { type: "nth" | "click"; i: number }
  /** any other key, a pointer down, a new query, the palette hidden */
  | { type: "key" | "pointer" | "query" | "hidden" };

/** The armed command's id, only while it is the selected row and the query `q` is the one it was armed under: a list
 * that changes under it (an app installed, the switch turned off) disarms it, so the prompt and Enter never point at
 * different rows, and so does a query change that sent no event (dictation, a context-menu paste). */
export function live(s: State, row: (i: number) => Row, q: string): string | null {
  const r = row(s.sel);
  return r && !isLead(r) && s.armed && r.id === s.armed.id && s.armed.q === q ? s.armed.id : null;
}

export type Out = State & {
  /** the row to run (a live result copies, an item launches), or null */
  run: number | null;
  /** the query replaces the current one: a history row was edited or its result inserted. The palette stays open. */
  query?: string;
};

/** What an event does to the selection and the armed command, which row it runs (`run`, or null) and whether it
 * replaces the query (`query`); `q` is the current query.
 * Enter or ⌘N or a click on a history row replaces the query with its expression, ⌘Enter appends its result: the
 * selection goes back to the first row and any armed command is disarmed, as for any query change. A held Enter's
 * repeats do nothing on a lead: the first press may have changed the query, and the repeat would land on its live row. */
export function step(s: State, ev: Event, row: (i: number) => Row, q: string): Out {
  /** Row `i` pressed while `before` is armed. */
  const act = (i: number, before: string | null, plain: boolean, meta = false): Out => {
    const r = row(i);
    if (r === undefined) return { armed: null, sel: s.sel, run: null };
    if (isLead(r)) {
      if (r.kind === "result") return { armed: null, sel: s.sel, run: i };
      return { armed: null, sel: 0, run: null, query: meta ? insert(q, r.result) : r.expr };
    }
    const p = press(before, r, plain);
    if (p.run) return { armed: null, sel: s.sel, run: i };
    // the armed row becomes the selected one, so the next Enter confirms it
    return p.armed ? { armed: { id: p.armed, q }, sel: i, run: null } : { armed: null, sel: s.sel, run: null };
  };
  switch (ev.type) {
    case "enter":
      if (ev.repeat && isLead(row(s.sel))) return { armed: null, sel: s.sel, run: null };
      // ⌘Enter, like ⌘N, can arm a command but never confirm one; a held Enter's repeats only disarm
      return ev.meta ? act(s.sel, null, true, true) : act(s.sel, live(s, row, q), !ev.repeat);
    case "nth":
    case "click":
      return act(ev.i, null, true);
    case "key":
    case "pointer":
      return { armed: null, sel: s.sel, run: null };
    case "query":
    case "hidden":
      return { armed: null, sel: 0, run: null };
  }
}
