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

/** A palette row: the calculator's answer, an item, or nothing (past the end). */
export type Row = Item | string | undefined;
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
  return typeof r === "object" && s.armed && r.id === s.armed.id && s.armed.q === q ? s.armed.id : null;
}

/** What an event does to the selection and the armed command, and which row it runs (`run`, or null); `q` is the
 * query. */
export function step(s: State, ev: Event, row: (i: number) => Row, q: string): State & { run: number | null } {
  /** Row `i` pressed while `before` is armed. */
  const act = (i: number, before: string | null, plain: boolean) => {
    const r = row(i);
    if (r === undefined) return { armed: null, sel: s.sel, run: null };
    if (typeof r === "string") return { armed: null, sel: s.sel, run: i };
    const p = press(before, r, plain);
    if (p.run) return { armed: null, sel: s.sel, run: i };
    // the armed row becomes the selected one, so the next Enter confirms it
    return p.armed ? { armed: { id: p.armed, q }, sel: i, run: null } : { armed: null, sel: s.sel, run: null };
  };
  switch (ev.type) {
    case "enter":
      // ⌘Enter, like ⌘N, can arm a command but never confirm one; a held Enter's repeats only disarm
      return ev.meta ? act(s.sel, null, true) : act(s.sel, live(s, row, q), !ev.repeat);
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
