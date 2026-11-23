import type { Item } from "./rank";

/**
 * Enter (or ⌘N, or a click) on `item` while the command `armed` waits for its second Enter. An item without a
 * `confirm` prompt runs. One with a prompt runs only on a `plain` press (a plain Enter, not ⌘Enter or a held Enter's
 * repeat) of the armed row; a plain press elsewhere arms it, anything else disarms.
 */
export function press(armed: string | null, item: Item, plain: boolean): { armed: string | null; run: boolean } {
  if (!item.confirm) return { armed: null, run: true };
  if (plain && armed === item.id) return { armed: null, run: true };
  return { armed: plain ? item.id : null, run: false };
}

/** The palette after a new query (or, with "", a hide): first row selected, no error, nothing armed. */
export const reset = (query: string) => ({ query, sel: 0, error: "", armed: null });
