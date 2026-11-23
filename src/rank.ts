import { fold, initials, tier } from "./fuzzy";

/** One item as `list_apps` / `apps-changed` send it (`apps::Item`). `id` is an app's path or `pane:<bundle id>`;
 * `path` is the bundle whose icon the row shows. */
export type Item = { kind: "app" | "pane" | "command"; id: string; name: string; altNames: string[]; path?: string; confirm?: string };

/** An item with its search keys folded once, when the list arrives, not on every keystroke. */
export type Prepared = { item: Item; keys: { f: string; ini: string }[] };

export const prepare = (items: Item[]): Prepared[] =>
  items.map((item) => ({
    item,
    keys: [item.name, ...item.altNames].map((k) => {
      const f = fold(k);
      return { f, ini: initials(f) };
    }),
  }));

/** Frecency per item id (`frecency` command). */
export type Frecency = Record<string, number>;

/** Learned picks (`picks` command): folded query → item id → decayed score of the times that item was opened for it. */
export type Picks = Record<string, Record<string, number>>;

/** Longest learned query; `picks.rs` caps its keys the same way. */
export const PICK_KEY = 30;

/** The key a pick is stored under: the folded, trimmed query, cut at `PICK_KEY` characters (a prefix, so it still applies). */
export const pickKey = (query: string): string => Array.from(fold(query.trim())).slice(0, PICK_KEY).join(""); // code points: never half an emoji

/** A pick score under this counts as none: `picks.rs` forgets it on load, and a file read before that could hold it. */
export const PICK_FORGET = 0.01;

/** Rows the empty query shows: the most used apps. */
export const RECENT = 6;
/** Rows a query renders at most; the card scrolls past the style's visible count. */
export const LIMIT = 20;

/** Under 1, so it only orders apps within one match tier: a perfect prefix always beats a frequent partial match. */
export const boost = (f = 0): number => (0.9 * f) / (f + 1);

/**
 * The palette's rows for `query`. Empty: up to `RECENT` apps and sections with any frecency (never commands), most
 * used first (nothing without history). Otherwise: items with a learned pick for `q` first, most picked first (the
 * summed scores under every key that is a prefix of `q`, so "me" → Meet also holds for "meet" but not for "m"). A
 * pick only counts for an item that matches `q` as a contiguous run (tier 2 and up): through scattered letters, a
 * pick for "s" would put Sleep first for "sp". Then best match tier over name and alt names, plus the frecency boost;
 * then an app before a command (a short command name must not win a tie: Enter on Sleep sleeps the Mac); then shorter name, then A–Z.
 */
export function search(query: string, items: Prepared[], frecency: Frecency, picks: Picks = {}, limit = LIMIT): Item[] {
  const q = fold(query.trim());
  const fr = (a: Item) => frecency[a.id] ?? 0;
  if (!q) {
    return items
      .map((i) => i.item)
      .filter((a) => fr(a) > 0 && a.kind !== "command") // nothing typed is no intent: one Enter must not lock or sleep
      .sort((a, b) => fr(b) - fr(a))
      .slice(0, RECENT);
  }
  const picked: Record<string, number> = {};
  for (let n = 1; n <= q.length; n++) for (const [id, v] of Object.entries(picks[q.slice(0, n)] ?? {})) picked[id] = (picked[id] ?? 0) + v;
  const hits: { item: Item; score: number; pick: number }[] = [];
  for (const { item, keys } of items) {
    let t = 0;
    for (const k of keys) t = Math.max(t, tier(q, k.f, k.ini));
    const p = t >= 2 ? (picked[item.id] ?? 0) : 0;
    if (t) hits.push({ item, score: t + boost(fr(item)), pick: p >= PICK_FORGET ? p : 0 });
  }
  return hits
    .sort((a, b) => b.pick - a.pick || b.score - a.score || +(a.item.kind === "command") - +(b.item.kind === "command") || a.item.name.length - b.item.name.length || a.item.name.localeCompare(b.item.name))
    .slice(0, limit)
    .map((h) => h.item);
}

/** Some item matches `query` strongly: exact, prefix, or word start (tier 4 and up). A query that starts with a symbol
 * has no words to start (`(` follows every space), so it needs a prefix. Only then does an open sum give up its history. */
export function strong(query: string, items: Prepared[]): boolean {
  const q = fold(query.trim());
  const min = /^[\p{L}\p{N}]/u.test(q) ? 4 : 5;
  return items.some(({ keys }) => keys.some((k) => tier(q, k.f, k.ini) >= min));
}

/** Selection after an arrow key: stays within the rows, 0 when there are none. */
export const move = (sel: number, delta: number, rows: number): number => Math.max(0, Math.min(rows - 1, sel + delta));
