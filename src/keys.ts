type Key = { key: string; isComposing: boolean; keyCode: number; preventDefault(): void };

/** The palette's window-level keys. On `window`, not the input: after a Tab the input would never see Esc. */
export function paletteKey(e: Key, hide: () => void) {
  if (e.key === "Tab") e.preventDefault(); // the input is the only focus target; leaving it strands typing
  // Esc while composing (IME) cancels the composition. macOS WebKit sends that keydown after compositionend with
  // isComposing false and keyCode 229 (WebKit bug 165004), so both are checked
  else if (e.key === "Escape" && !e.isComposing && e.keyCode !== 229) hide();
}
