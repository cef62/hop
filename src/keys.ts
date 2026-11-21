type Key = { key: string; isComposing: boolean; preventDefault(): void };

/** The palette's window-level keys. On `window`, not the input: after a Tab the input would never see Esc. */
export function paletteKey(e: Key, hide: () => void) {
  if (e.key === "Tab") e.preventDefault(); // the input is the only focus target; leaving it strands typing
  else if (e.key === "Escape" && !e.isComposing) hide(); // Esc while composing (IME) cancels the composition
}
