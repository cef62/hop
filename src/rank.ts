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

/** Rows the empty query shows: the most used apps. */
export const RECENT = 6;
/** Rows a query renders at most; the card scrolls past the style's visible count. */
export const LIMIT = 20;

/** Under 1, so it only orders apps within one match tier: a perfect prefix always beats a frequent partial match. */
export const boost = (f = 0): number => (0.9 * f) / (f + 1);

/**
 * The palette's rows for `query`. Empty: up to `RECENT` apps with any frecency, most used first (nothing without
 * history). Otherwise: best match tier over name and alt names, plus the frecency boost; then shorter name, then A–Z.
 */
export function search(query: string, items: Prepared[], frecency: Frecency, limit = LIMIT): Item[] {
  const q = fold(query.trim());
  const fr = (a: Item) => frecency[a.id] ?? 0;
  if (!q) {
    return items
      .map((i) => i.item)
      .filter((a) => fr(a) > 0)
      .sort((a, b) => fr(b) - fr(a))
      .slice(0, RECENT);
  }
  const hits: { item: Item; score: number }[] = [];
  for (const { item, keys } of items) {
    let t = 0;
    for (const k of keys) t = Math.max(t, tier(q, k.f, k.ini));
    if (t) hits.push({ item, score: t + boost(fr(item)) });
  }
  return hits
    .sort((a, b) => b.score - a.score || a.item.name.length - b.item.name.length || a.item.name.localeCompare(b.item.name))
    .slice(0, limit)
    .map((h) => h.item);
}

/** Selection after an arrow key: stays within the rows, 0 when there are none. */
export const move = (sel: number, delta: number, rows: number): number => Math.max(0, Math.min(rows - 1, sel + delta));
