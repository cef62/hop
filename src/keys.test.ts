import { expect, test, vi } from "vitest";
import { paletteKey } from "./keys";

const ev = (key: string, isComposing = false, keyCode = 0) => ({ key, isComposing, keyCode, preventDefault: vi.fn() });

test("Esc hides from any element, Tab never moves focus off the input", () => {
  const hide = vi.fn();
  paletteKey(ev("Escape"), hide);
  expect(hide).toHaveBeenCalledOnce();
  const tab = ev("Tab");
  paletteKey(tab, hide);
  expect(tab.preventDefault).toHaveBeenCalled();
  expect(hide).toHaveBeenCalledOnce();
});

test("Esc during IME composition cancels the composition, not the palette", () => {
  const hide = vi.fn();
  paletteKey(ev("Escape", true), hide);
  paletteKey(ev("Escape", false, 229), hide); // macOS WebKit: the cancelling keydown arrives after compositionend
  expect(hide).not.toHaveBeenCalled();
});
