import { expect, test, vi } from "vitest";
import { paletteKey } from "./keys";

const ev = (key: string, isComposing = false, keyCode = 0, metaKey = false, code = "", shiftKey = false, repeat = false) => ({ key, code, isComposing, keyCode, metaKey, shiftKey, repeat, preventDefault: vi.fn() });
const actions = () => ({ hide: vi.fn(), move: vi.fn(), open: vi.fn(), openAt: vi.fn(), settings: vi.fn(), update: vi.fn(), disarm: vi.fn() });

test("Esc hides from any element, Tab never moves focus off the input", () => {
  const a = actions();
  paletteKey(ev("Escape"), a);
  expect(a.hide).toHaveBeenCalledOnce();
  const tab = ev("Tab");
  paletteKey(tab, a);
  expect(tab.preventDefault).toHaveBeenCalled();
  expect(a.hide).toHaveBeenCalledOnce();
});

test("arrows move the selection, Enter opens, ⌘Enter reveals", () => {
  const a = actions();
  paletteKey(ev("ArrowDown"), a);
  paletteKey(ev("ArrowUp"), a);
  expect(a.move.mock.calls).toEqual([[1], [-1]]);
  paletteKey(ev("Enter"), a);
  paletteKey(ev("Enter", false, 0, true), a);
  expect(a.open.mock.calls).toEqual([[false, false], [true, false]]);
  // a held Enter's repeats are told apart: they must not confirm an armed command
  paletteKey(ev("Enter", false, 0, false, "", false, true), a);
  expect(a.open).toHaveBeenLastCalledWith(false, true);
});

test("any other key disarms", () => {
  const disarming = [ev("a"), ev("ArrowDown"), ev("ArrowUp"), ev("Escape"), ev("Tab"), ev("Backspace"), ev(" "), ev("1", false, 0, true, "Digit1"), ev("v", false, 0, true), ev(",", false, 0, true), ev("x", true)];
  for (const e of disarming) {
    const a = actions();
    paletteKey(e, a);
    expect(a.disarm, e.key).toHaveBeenCalledOnce();
  }
  // Enter confirms or arms itself; a bare modifier is half of a chord (⌘ shows the ⌘N hints)
  for (const e of [ev("Enter"), ev("Enter", false, 0, true), ev("Meta", false, 0, true), ev("Shift"), ev("Control"), ev("Alt"), ev("CapsLock")]) {
    const a = actions();
    paletteKey(e, a);
    expect(a.disarm, e.key).not.toHaveBeenCalled();
  }
});

test("keys during IME composition belong to the IME, not the palette", () => {
  const a = actions();
  for (const k of ["Escape", "Enter", "ArrowDown", "ArrowUp"]) {
    paletteKey(ev(k, true), a);
    paletteKey(ev(k, false, 229), a); // macOS WebKit: the cancelling keydown arrives after compositionend
  }
  expect(a.hide).not.toHaveBeenCalled();
  expect(a.move).not.toHaveBeenCalled();
  expect(a.open).not.toHaveBeenCalled();
});

test("⌘, opens Settings, a plain comma is typed", () => {
  const a = actions();
  const plain = ev(",");
  paletteKey(plain, a);
  expect(a.settings).not.toHaveBeenCalled();
  expect(plain.preventDefault).not.toHaveBeenCalled();
  paletteKey(ev(",", false, 0, true), a);
  expect(a.settings).toHaveBeenCalledOnce();
});

test("⌘U opens the update pill, a plain u is typed", () => {
  const a = actions();
  const plain = ev("u");
  paletteKey(plain, a);
  expect(a.update).not.toHaveBeenCalled();
  expect(plain.preventDefault).not.toHaveBeenCalled();
  paletteKey(ev("u", false, 0, true), a);
  paletteKey(ev("U", false, 0, true), a); // caps lock
  expect(a.update).toHaveBeenCalledTimes(2);
});

test("⌘1…⌘9 open that row, ⌘⇧digit is left to the system", () => {
  const a = actions();
  const one = ev("1", false, 0, true, "Digit1");
  paletteKey(one, a);
  const shot = ev("#", false, 0, true, "Digit3", true); // macOS screenshot shortcut
  paletteKey(shot, a);
  expect(shot.preventDefault).not.toHaveBeenCalled();
  paletteKey(ev("9", false, 0, true, "Digit9"), a);
  expect(a.openAt.mock.calls).toEqual([[0], [8]]);
  expect(one.preventDefault).toHaveBeenCalled();
});

test("a digit without ⌘ is typed, ⌘0 is not ours", () => {
  const a = actions();
  const typed = ev("1", false, 0, false, "Digit1");
  paletteKey(typed, a);
  const zero = ev("0", false, 0, true, "Digit0");
  paletteKey(zero, a);
  expect(a.openAt).not.toHaveBeenCalled();
  expect(typed.preventDefault).not.toHaveBeenCalled();
  expect(zero.preventDefault).not.toHaveBeenCalled();
});

test("AZERTY: ⌘& is Digit1", () => {
  const a = actions();
  paletteKey(ev("&", false, 0, true, "Digit1"), a);
  expect(a.openAt).toHaveBeenCalledWith(0);
});

test("⌘1 during IME composition belongs to the IME", () => {
  const a = actions();
  paletteKey(ev("1", true, 0, true, "Digit1"), a);
  paletteKey(ev("1", false, 229, true, "Digit1"), a);
  expect(a.openAt).not.toHaveBeenCalled();
});
