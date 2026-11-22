import { expect, test, vi } from "vitest";
import { paletteKey } from "./keys";

const ev = (key: string, isComposing = false, keyCode = 0, metaKey = false) => ({ key, isComposing, keyCode, metaKey, preventDefault: vi.fn() });
const actions = () => ({ hide: vi.fn(), move: vi.fn(), open: vi.fn(), settings: vi.fn() });

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
  expect(a.open.mock.calls).toEqual([[false], [true]]);
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
