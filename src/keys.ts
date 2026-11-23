type Key = { key: string; code: string; shiftKey: boolean; isComposing: boolean; keyCode: number; metaKey: boolean; repeat: boolean; preventDefault(): void };

export type Actions = {
  hide(): void;
  move(delta: number): void;
  /** `repeat`: a held Enter's auto-repeat, which must not confirm what its first keydown armed */
  open(reveal: boolean, repeat: boolean): void;
  openAt(index: number): void;
  settings(): void;
  update(): void;
  /** forgets a command waiting for its second Enter */
  disarm(): void;
};

/** Keys that are only half of a chord: holding ⌘ to see the ⌘N hints keeps an armed command armed. */
const MODIFIERS = ["Meta", "Shift", "Control", "Alt", "CapsLock"];

/** The palette's window-level keys. On `window`, not the input: after a Tab the input would never see Esc. */
export function paletteKey(e: Key, a: Actions) {
  if (e.key !== "Enter" && !MODIFIERS.includes(e.key)) a.disarm(); // only a second Enter confirms
  if (e.key === "Tab") e.preventDefault(); // the input is the only focus target; leaving it strands typing
  // While composing (IME), Esc cancels the composition, Enter commits it and the arrows pick a candidate: none of them
  // is ours. macOS WebKit sends the cancelling Esc after compositionend with isComposing false and keyCode 229
  // (WebKit bug 165004), so both are checked
  else if (e.isComposing || e.keyCode === 229) return;
  else if (e.key === "Escape") a.hide();
  else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault(); // the caret stays put
    a.move(e.key === "ArrowDown" ? 1 : -1);
  } else if (e.key === "Enter") {
    e.preventDefault();
    a.open(e.metaKey, e.repeat); // ⌘Enter reveals in Finder
  } else if (e.metaKey && !e.shiftKey && /^Digit[1-9]$/.test(e.code)) {
    // `code`, not `key`: on AZERTY the digit row types symbols unless ⇧ is held. ⌘⇧3/4/5 are macOS screenshots, so ⇧ is not ours
    e.preventDefault();
    a.openAt(Number(e.code.slice(5)) - 1);
  } else if (e.metaKey && e.key === ",") {
    e.preventDefault();
    a.settings();
  } else if (e.metaKey && e.key.toLowerCase() === "u") {
    e.preventDefault();
    a.update(); // the footer pill, when an update is ready
  }
}
