import { expect, test, vi } from "vitest";
import { live, press, step, type Event, type State } from "./confirm";
import { rowAt, type Lead, type Row } from "./leads";
import { paletteKey } from "./keys";
import type { Item } from "./rank";

const cmd = (id: string, confirm?: string): Item => ({ kind: "command", id: `cmd:${id}`, name: id, altNames: [], confirm });
const restart = cmd("restart", "Press Enter again to restart");
const shutdown = cmd("shutdown", "Press Enter again to shut down");
const lock = cmd("lock");
const app: Item = { kind: "app", id: "/Applications/Safari.app", name: "Safari", altNames: [], path: "/Applications/Safari.app" };

test("an item without a confirm prompt runs at once and never arms", () => {
  for (const item of [lock, cmd("sleep"), app]) {
    expect(press(null, item, true)).toEqual({ armed: null, run: true });
    expect(press(null, item, false)).toEqual({ armed: null, run: true });
    // something else was armed: running this disarms it
    expect(press(restart.id, item, true)).toEqual({ armed: null, run: true });
  }
});

test("the first plain Enter arms, the second on the same row runs", () => {
  const first = press(null, restart, true);
  expect(first).toEqual({ armed: restart.id, run: false });
  expect(press(first.armed, restart, true)).toEqual({ armed: null, run: true });
});

test("⌘Enter and a held Enter's repeats never confirm: they disarm", () => {
  expect(press(restart.id, restart, false)).toEqual({ armed: null, run: false });
  expect(press(null, restart, false)).toEqual({ armed: null, run: false });
});

test("arming a second command while one is armed arms only the second", () => {
  expect(press(restart.id, shutdown, true)).toEqual({ armed: shutdown.id, run: false });
  // an armed id that is no longer in the list (the switch turned commands off) never runs anything
  expect(press("cmd:gone", restart, true)).toEqual({ armed: restart.id, run: false });
});

/** The palette's rows: the calculator's answer, then Restart, Lock Screen, Safari. */
const answer: Lead = { kind: "result", text: "42" };
const list: Row[] = [answer, restart, lock, app];
const row = (i: number) => list[i];
const idle = (sel: number): State => ({ armed: null, sel });
/** Restart armed at row 1, under the query "re". */
const A = { id: restart.id, q: "re" };
const armedAt1: State = { armed: A, sel: 1 };
const go = (s: State, ev: Event, at = row, q = "re") => step(s, ev, at, q);
const alive = (s: State, at: (i: number) => Row, q = "re") => live(s, at, q);
const enter = { type: "enter" as const, meta: false, repeat: false };

test("Enter arms a confirm row, a second plain Enter runs it", () => {
  expect(go(idle(1), enter, row)).toEqual({ armed: A, sel: 1, run: null });
  expect(go(armedAt1, enter, row)).toEqual({ armed: null, sel: 1, run: 1 });
  // rows without a prompt run on the first Enter: the answer, Lock Screen, an app
  for (const i of [0, 2, 3]) expect(go(idle(i), enter, row)).toEqual({ armed: null, sel: i, run: i });
  // past the end (no rows): nothing
  expect(go(idle(9), enter, row)).toEqual({ armed: null, sel: 9, run: null });
});

test("a held Enter's repeats never confirm: they disarm", () => {
  expect(go(armedAt1, { ...enter, repeat: true }, row)).toEqual({ armed: null, sel: 1, run: null });
  expect(go(idle(1), { ...enter, repeat: true }, row)).toEqual({ armed: null, sel: 1, run: null });
});

test("⌘Enter, ⌘N and a click arm a confirm row and select it, but never confirm", () => {
  for (const s of [idle(1), armedAt1]) expect(go(s, { ...enter, meta: true }, row)).toEqual({ armed: A, sel: 1, run: null });
  for (const type of ["nth", "click"] as const) {
    expect(go(idle(3), { type, i: 1 }, row)).toEqual({ armed: A, sel: 1, run: null });
    expect(go(armedAt1, { type, i: 1 }, row)).toEqual({ armed: A, sel: 1, run: null });
    // elsewhere they run their row and leave the selection alone
    expect(go(armedAt1, { type, i: 3 }, row)).toEqual({ armed: null, sel: 1, run: 3 });
  }
  // ⌘Enter on an app reveals it (the caller passes reveal), on Lock Screen it runs
  expect(go(idle(3), { ...enter, meta: true }, row)).toEqual({ armed: null, sel: 3, run: 3 });
  expect(go(idle(2), { ...enter, meta: true }, row)).toEqual({ armed: null, sel: 2, run: 2 });
});

test("any other key and a pointer down disarm in place; a new query and a hide also go back to the first row", () => {
  for (const type of ["key", "pointer"] as const) expect(go(armedAt1, { type }, row)).toEqual({ armed: null, sel: 1, run: null });
  for (const type of ["query", "hidden"] as const) expect(go(armedAt1, { type }, row)).toEqual({ armed: null, sel: 0, run: null });
});

test("armed only while the armed command is the selected row", () => {
  expect(alive(armedAt1, row)).toBe(restart.id);
  // the query changed without a "query" event (dictation, a context-menu paste): Restart is still row 1, but disarmed
  expect(alive(armedAt1, row, "restart")).toBeNull();
  expect(go(armedAt1, enter, row, "restart")).toEqual({ armed: { id: restart.id, q: "restart" }, sel: 1, run: null });
  expect(alive({ armed: A, sel: 2 }, row)).toBeNull();
  // an app installed meanwhile takes row 1: Enter opens it, nothing restarts
  const moved: Row[] = [answer, app, restart, lock];
  const at = (i: number) => moved[i];
  expect(alive(armedAt1, at)).toBeNull();
  expect(go(armedAt1, enter, at)).toEqual({ armed: null, sel: 1, run: 1 });
  // the switch turned commands off: the armed id is gone
  expect(alive(armedAt1, (i) => [app, lock][i])).toBeNull();
  expect(alive({ armed: A, sel: 0 }, row)).toBeNull(); // the answer row is never armed
});

test('"re" Enter, then "a": the key and the query change disarm, so the next Enter opens the new top row', () => {
  let s: State = go(idle(1), enter, row);
  expect(s.armed).toEqual(A);
  const send = vi.fn((ev: Event) => (s = go(s, ev)));
  const actions = { hide() {}, move() {}, settings() {}, update() {}, send };
  paletteKey({ key: "a", code: "KeyA", shiftKey: false, isComposing: false, keyCode: 65, metaKey: false, repeat: false, preventDefault() {} }, actions);
  expect(send).toHaveBeenCalledExactlyOnceWith({ type: "key" });
  expect(s.armed).toBeNull();
  // a paste or dictation sends no key: the query change alone disarms
  s = go(armedAt1, { type: "query" }, row);
  const rea: Row[] = [app, restart];
  expect(go(s, enter, (i) => rea[i])).toEqual({ armed: null, sel: 0, run: 0 });
});

// ---- calculator history rows ----
const old: Lead = { kind: "history", expr: "12*7+3", result: "87" };
const big: Lead = { kind: "history", expr: "2^64", result: "1.84467440737e+19" };
/** `=`: two history rows, then Restart and an app */
const eq: Row[] = [old, big, restart, app];
const onEq = (i: number) => eq[i];
const gq = (s: State, ev: Event, q = "=") => step(s, ev, onEq, q);

test("Enter on a history row replaces the query with its expression, selects the first row, runs nothing", () => {
  expect(gq(idle(0), enter)).toEqual({ armed: null, sel: 0, run: null, query: "12*7+3" });
  expect(gq(idle(1), enter)).toEqual({ armed: null, sel: 0, run: null, query: "2^64" });
});

test("⌘N and a click on a history row edit it too, from any selection", () => {
  for (const type of ["nth", "click"] as const) {
    expect(gq(idle(3), { type, i: 1 })).toEqual({ armed: null, sel: 0, run: null, query: "2^64" });
    expect(gq(idle(0), { type, i: 0 })).toEqual({ armed: null, sel: 0, run: null, query: "12*7+3" });
  }
});

test("⌘Enter on a history row inserts its result where the query has a place for it, exponent form as it is", () => {
  const meta = { ...enter, meta: true };
  const none = { armed: null, sel: 0, run: null };
  expect(gq(idle(0), meta, "1+")).toEqual({ ...none, query: "1+87" });
  expect(gq(idle(1), meta, "1+")).toEqual({ ...none, query: "1+1.84467440737e+19" });
  expect(gq(idle(0), meta, "=")).toEqual({ ...none, query: "87" });
  expect(gq(idle(0), meta, "")).toEqual({ ...none, query: "87" });
  // a complete sum or a number has no place for it: nothing happens, the selection stays
  for (const q of ["12", "1+2", "(1+2)"]) expect(gq(idle(1), meta, q)).toEqual({ armed: null, sel: 1, run: null });
});

test("⌘Enter on the live result copies it, as Enter does", () => {
  const live1: Row[] = [answer, old];
  for (const meta of [true, false]) expect(step(idle(0), { ...enter, meta }, (i) => live1[i], "6*7")).toEqual({ armed: null, sel: 0, run: 0 });
  // a history row under a complete sum: ⌘Enter does nothing there
  expect(step(idle(1), { ...enter, meta: true }, (i) => live1[i], "6*7")).toEqual({ armed: null, sel: 1, run: null });
});

test("a held Enter does nothing on a lead: not an edit, not a copy, not the live row the first press made", () => {
  for (const meta of [false, true]) {
    for (const sel of [0, 1]) expect(gq(idle(sel), { ...enter, repeat: true, meta })).toEqual({ armed: null, sel, run: null });
    expect(step(idle(0), { ...enter, repeat: true, meta }, (i) => [answer, old][i], "6*7")).toEqual({ armed: null, sel: 0, run: null });
  }
  // the first press, then the auto-repeat on the new query: one edit, nothing copied
  let s: State = idle(0);
  const first = gq(s, enter);
  expect(first.query).toBe("12*7+3");
  const after: Row[] = [{ kind: "result", text: "87" }, restart];
  expect(step({ armed: first.armed, sel: first.sel }, { ...enter, repeat: true }, (i) => after[i], first.query!).run).toBeNull();
});

test("a command armed when a history row replaces the query is disarmed", () => {
  const armedUnderEq: State = { armed: { id: restart.id, q: "=" }, sel: 2 };
  expect(alive(armedUnderEq, onEq, "=")).toBe(restart.id);
  // Enter, ⌘N, ⌘Enter and a click on a history row: the query changes, so nothing stays armed
  const onHistory: State = { armed: armedUnderEq.armed, sel: 0 }; // armed, but the selection moved to a history row
  const cases: [State, Event][] = [
    [onHistory, enter],
    [onHistory, { ...enter, meta: true }],
    [armedUnderEq, { type: "nth", i: 0 }],
    [armedUnderEq, { type: "click", i: 1 }],
  ];
  for (const [from, ev] of cases) {
    const o = gq(from, ev);
    expect(o.armed).toBeNull();
    expect(o.query).toBeDefined();
    expect(o.run).toBeNull();
    // and the new query is not the one Restart was armed under
    expect(alive({ armed: o.armed, sel: o.sel }, onEq, o.query!)).toBeNull();
  }
  // even if a stale armed state survived (no event), the query check disarms it
  expect(alive(armedUnderEq, onEq, "=87")).toBeNull();
  // and the armed state of a different row never confirms by way of a lead: Enter on row 1 (history) edits, not runs Restart
  expect(gq({ armed: { id: restart.id, q: "=" }, sel: 1 }, enter).run).toBeNull();
});

test("leads are never armed; a click on a lead leaves the selection to the new query", () => {
  expect(alive({ armed: { id: old.expr, q: "=" }, sel: 0 }, onEq, "=")).toBeNull();
  expect(rowAt(0, [old, big], [app])).toBe(old);
});

test("a held ⌘N does nothing: not an edit, not a copy of the live row the first press made, not an item", () => {
  const rep = (i: number): Event => ({ type: "nth", i, repeat: true });
  for (const i of [0, 1, 2, 3]) expect(gq(idle(0), rep(i))).toEqual({ armed: null, sel: 0, run: null });
  expect(gq(armedAt1, rep(1))).toEqual({ armed: null, sel: 1, run: null }); // not even an arming
  // the first press edits; the repeat lands on the new query's live row and does not copy it
  expect(gq(idle(0), { type: "nth", i: 0, repeat: false })).toEqual({ armed: null, sel: 0, run: null, query: "12*7+3" });
  const after: Row[] = [{ kind: "result", text: "87" }, old, restart];
  expect(step(idle(0), rep(0), (i) => after[i], "12*7+3")).toEqual({ armed: null, sel: 0, run: null });
});
