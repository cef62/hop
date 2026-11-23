import { expect, test, vi } from "vitest";
import { press, reset } from "./confirm";
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

test("a new query or a hide leaves nothing armed", () => {
  expect(reset("rea")).toEqual({ query: "rea", sel: 0, error: "", armed: null });
  expect(reset("").armed).toBeNull();
});

test('"re" Enter, then "a": the query change disarms, so the next Enter opens the new top row', () => {
  let armed: string | null = press(null, restart, true).armed;
  expect(armed).toBe(restart.id);
  const disarm = vi.fn(() => (armed = null));
  const actions = { hide() {}, move() {}, open() {}, openAt() {}, settings() {}, update() {}, disarm };
  paletteKey({ key: "a", code: "KeyA", shiftKey: false, isComposing: false, keyCode: 65, metaKey: false, repeat: false, preventDefault() {} }, actions);
  expect(disarm).toHaveBeenCalledOnce();
  armed = reset("rea").armed; // and onQuery, for a paste or dictation that sends no key
  expect(press(armed, app, true)).toEqual({ armed: null, run: true });
  expect(press(armed, restart, true)).toEqual({ armed: restart.id, run: false }); // Restart still on top: arms again
});
