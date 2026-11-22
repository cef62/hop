import { fold, initials, tier } from "./fuzzy";

/** One app as `list_apps` / `apps-changed` send it. */
export type App = { name: string; altNames: string[]; path: string };

/** An app with its search keys folded once, when the list arrives, not on every keystroke. */
export type Prepared = { app: App; keys: { f: string; ini: string }[] };

export const prepare = (apps: App[]): Prepared[] =>
  apps.map((app) => ({
    app,
    keys: [app.name, ...app.altNames].map((k) => {
      const f = fold(k);
      return { f, ini: initials(f) };
    }),
  }));

/** Frecency per app path (`frecency` command). */
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
export function search(query: string, items: Prepared[], frecency: Frecency, limit = LIMIT): App[] {
  const q = fold(query.trim());
  const fr = (a: App) => frecency[a.path] ?? 0;
  if (!q) {
    return items
      .map((i) => i.app)
      .filter((a) => fr(a) > 0)
      .sort((a, b) => fr(b) - fr(a))
      .slice(0, RECENT);
  }
  const hits: { app: App; score: number }[] = [];
  for (const { app, keys } of items) {
    let t = 0;
    for (const k of keys) t = Math.max(t, tier(q, k.f, k.ini));
    if (t) hits.push({ app, score: t + boost(fr(app)) });
  }
  return hits
    .sort((a, b) => b.score - a.score || a.app.name.length - b.app.name.length || a.app.name.localeCompare(b.app.name))
    .slice(0, limit)
    .map((h) => h.app);
}

/** Selection after an arrow key: stays within the rows, 0 when there are none. */
export const move = (sel: number, delta: number, rows: number): number => Math.max(0, Math.min(rows - 1, sel + delta));
