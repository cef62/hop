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
/** The selected row and the command waiting for its second Enter. */
export type State = { armed: string | null; sel: number };
export type Event =
  | { type: "enter"; meta: boolean; repeat: boolean }
  /** ⌘N, or a click on a row */
  | { type: "nth" | "click"; i: number }
  /** any other key, a pointer down, a new query, the palette hidden */
  | { type: "key" | "pointer" | "query" | "hidden" };

/** The armed command, only while it is the selected row: a list that changes under it (an app installed, the switch
 * turned off) disarms it, so the prompt and Enter never point at different rows. */
export function live(s: State, row: (i: number) => Row): string | null {
  const r = row(s.sel);
  return typeof r === "object" && r.id === s.armed ? s.armed : null;
}

/** What an event does to the selection and the armed command, and which row it runs (`run`, or null). */
export function step(s: State, ev: Event, row: (i: number) => Row): State & { run: number | null } {
  /** Row `i` pressed while `before` is armed. */
  const act = (i: number, before: string | null, plain: boolean) => {
    const r = row(i);
    if (r === undefined) return { armed: null, sel: s.sel, run: null };
    if (typeof r === "string") return { armed: null, sel: s.sel, run: i };
    const p = press(before, r, plain);
    // the armed row becomes the selected one, so the next Enter confirms it
    return p.run ? { armed: null, sel: s.sel, run: i } : { armed: p.armed, sel: p.armed ? i : s.sel, run: null };
  };
  switch (ev.type) {
    case "enter":
      // ⌘Enter, like ⌘N, can arm a command but never confirm one; a held Enter's repeats only disarm
      return ev.meta ? act(s.sel, null, true) : act(s.sel, live(s, row), !ev.repeat);
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
