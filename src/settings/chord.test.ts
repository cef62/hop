import { expect, test } from "vitest";
import { formatChord, recorderStep } from "./chord";

const key = (code: string, mods: Partial<Record<"metaKey" | "ctrlKey" | "altKey" | "shiftKey", boolean>> = {}) => ({
  code,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

test("a chord with a modifier ends recording in global-shortcut syntax", () => {
  expect(recorderStep(key("Space", { altKey: true }))).toEqual({ recording: false, chord: "Alt+Space" });
  expect(recorderStep(key("KeyK", { metaKey: true, ctrlKey: true, altKey: true, shiftKey: true }))).toEqual({
    recording: false,
    chord: "Control+Alt+Shift+Super+KeyK",
  });
  expect(recorderStep(key("F5"))).toEqual({ recording: false, chord: "F5" }); // function keys need no modifier
});

test("modifiers alone keep recording, Esc cancels, Backspace clears", () => {
  for (const c of ["AltLeft", "MetaRight", "ShiftLeft", "ControlLeft", ""]) expect(recorderStep(key(c, { altKey: true }))).toEqual({ recording: true });
  expect(recorderStep(key("Escape"))).toEqual({ recording: false });
  expect(recorderStep(key("Backspace"))).toEqual({ recording: false, chord: "" });
  expect(recorderStep(key("Delete"))).toEqual({ recording: false, chord: "" });
  // with a modifier they are chords like any other
  expect(recorderStep(key("Backspace", { altKey: true }))).toEqual({ recording: false, chord: "Alt+Backspace" });
});

test("a plain or shifted key is refused: it would take over typing", () => {
  for (const k of [key("KeyA"), key("KeyA", { shiftKey: true }), key("Space")]) {
    const s = recorderStep(k);
    expect(s.recording).toBe(true);
    expect(s.chord).toBeUndefined();
    expect(s.msg).toMatch(/⌘, ⌥ or ⌃/);
  }
});

test("chords read the macOS way", () => {
  expect(formatChord("Alt+Space")).toBe("⌥Space");
  expect(formatChord("Control+Alt+Shift+Super+KeyK")).toBe("⌃⌥⇧⌘K");
  expect(formatChord("Super+Digit1")).toBe("⌘1");
  expect(formatChord("Super+ArrowUp")).toBe("⌘↑");
  expect(formatChord("Alt+Backquote")).toBe("⌥`");
  expect(formatChord("")).toBe("None");
});
