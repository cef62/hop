import { expect, test, vi } from "vitest";
import { live, press, step, type Event, type Row, type State } from "./confirm";
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
const list: Row[] = ["42", restart, lock, app];
const row = (i: number) => list[i];
const idle = (sel: number): State => ({ armed: null, sel });
const armedAt1: State = { armed: restart.id, sel: 1 };
const enter = { type: "enter" as const, meta: false, repeat: false };

test("Enter arms a confirm row, a second plain Enter runs it", () => {
  expect(step(idle(1), enter, row)).toEqual({ armed: restart.id, sel: 1, run: null });
  expect(step(armedAt1, enter, row)).toEqual({ armed: null, sel: 1, run: 1 });
  // rows without a prompt run on the first Enter: the answer, Lock Screen, an app
  for (const i of [0, 2, 3]) expect(step(idle(i), enter, row)).toEqual({ armed: null, sel: i, run: i });
  // past the end (no rows): nothing
  expect(step(idle(9), enter, row)).toEqual({ armed: null, sel: 9, run: null });
});

test("a held Enter's repeats never confirm: they disarm", () => {
  expect(step(armedAt1, { ...enter, repeat: true }, row)).toEqual({ armed: null, sel: 1, run: null });
  expect(step(idle(1), { ...enter, repeat: true }, row)).toEqual({ armed: null, sel: 1, run: null });
});

test("⌘Enter, ⌘N and a click arm a confirm row and select it, but never confirm", () => {
  for (const s of [idle(1), armedAt1]) expect(step(s, { ...enter, meta: true }, row)).toEqual({ armed: restart.id, sel: 1, run: null });
  for (const type of ["nth", "click"] as const) {
    expect(step(idle(3), { type, i: 1 }, row)).toEqual({ armed: restart.id, sel: 1, run: null });
    expect(step(armedAt1, { type, i: 1 }, row)).toEqual({ armed: restart.id, sel: 1, run: null });
    // elsewhere they run their row and leave the selection alone
    expect(step(armedAt1, { type, i: 3 }, row)).toEqual({ armed: null, sel: 1, run: 3 });
  }
  // ⌘Enter on an app reveals it (the caller passes reveal), on Lock Screen it runs
  expect(step(idle(3), { ...enter, meta: true }, row)).toEqual({ armed: null, sel: 3, run: 3 });
  expect(step(idle(2), { ...enter, meta: true }, row)).toEqual({ armed: null, sel: 2, run: 2 });
});

test("any other key and a pointer down disarm in place; a new query and a hide also go back to the first row", () => {
  for (const type of ["key", "pointer"] as const) expect(step(armedAt1, { type }, row)).toEqual({ armed: null, sel: 1, run: null });
  for (const type of ["query", "hidden"] as const) expect(step(armedAt1, { type }, row)).toEqual({ armed: null, sel: 0, run: null });
});

test("armed only while the armed command is the selected row", () => {
  expect(live(armedAt1, row)).toBe(restart.id);
  expect(live({ armed: restart.id, sel: 2 }, row)).toBeNull();
  // an app installed meanwhile takes row 1: Enter opens it, nothing restarts
  const moved: Row[] = ["42", app, restart, lock];
  const at = (i: number) => moved[i];
  expect(live(armedAt1, at)).toBeNull();
  expect(step(armedAt1, enter, at)).toEqual({ armed: null, sel: 1, run: 1 });
  // the switch turned commands off: the armed id is gone
  expect(live(armedAt1, (i) => [app, lock][i])).toBeNull();
  expect(live({ armed: restart.id, sel: 0 }, row)).toBeNull(); // the answer row is never armed
});

test('"re" Enter, then "a": the key and the query change disarm, so the next Enter opens the new top row', () => {
  let s: State = step(idle(1), enter, row);
  expect(s.armed).toBe(restart.id);
  const send = vi.fn((ev: Event) => (s = step(s, ev, row)));
  const actions = { hide() {}, move() {}, settings() {}, update() {}, send };
  paletteKey({ key: "a", code: "KeyA", shiftKey: false, isComposing: false, keyCode: 65, metaKey: false, repeat: false, preventDefault() {} }, actions);
  expect(send).toHaveBeenCalledExactlyOnceWith({ type: "key" });
  expect(s.armed).toBeNull();
  // a paste or dictation sends no key: the query change alone disarms
  s = step(armedAt1, { type: "query" }, row);
  const rea: Row[] = [app, restart];
  expect(step(s, enter, (i) => rea[i])).toEqual({ armed: null, sel: 0, run: 0 });
});
