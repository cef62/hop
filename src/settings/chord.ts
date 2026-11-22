/** The hotkey recorder, ported from velo's: one keydown in, the next state out. Chords are in the global-shortcut
 * plugin's syntax (modifiers, then `KeyboardEvent.code`), which is what settings.json stores. */

type Ev = Pick<KeyboardEvent, "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey">;

/** `recording: false` ends recording; `chord` (empty: no hotkey) is the new value to save; `msg` explains a refusal. */
export type Step = { recording: boolean; chord?: string; msg?: string };

const MODIFIER = /^(Meta|Control|Alt|Shift|OS|CapsLock|Fn)/;
const FKEY = /^F([1-9]|1[0-9]|20)$/;

export function recorderStep(e: Ev): Step {
  if (e.code === "Escape") return { recording: false };
  if (!e.code || MODIFIER.test(e.code)) return { recording: true }; // a modifier on its own: keep listening
  const mods = e.metaKey || e.ctrlKey || e.altKey;
  if (!mods && !e.shiftKey && (e.code === "Backspace" || e.code === "Delete")) return { recording: false, chord: "" };
  if (!mods && !FKEY.test(e.code)) return { recording: true, msg: "Add ⌘, ⌥ or ⌃: without one the shortcut would take over typing." };
  const parts = [e.ctrlKey && "Control", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super", e.code];
  return { recording: false, chord: parts.filter(Boolean).join("+") };
}

const GLYPHS: Record<string, string> = {
  Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘",
  ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Backspace: "⌫", Enter: "↩", Tab: "⇥",
  Backquote: "`", Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\", Semicolon: ";",
  Quote: "'", Comma: ",", Period: ".", Slash: "/",
};

/** "Control+Alt+Shift+Super+KeyK" → "⌃⌥⇧⌘K" (Apple's modifier order, which the recorder already writes). */
export function formatChord(chord: string): string {
  if (!chord) return "None";
  return chord
    .split("+")
    .map((p) => GLYPHS[p] ?? p.replace(/^(Key|Digit)/, ""))
    .join("");
}
