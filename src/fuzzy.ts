// Ported from velo's src/fuzzy.ts, plus a word-initials tier ("vsc" → "Visual Studio Code").

/** Lowercase without diacritics: "Résumé" → "resume". */
export const fold = (s: string): string => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

const WORD = /[\p{L}\p{N}]/u;

/** First letter of each word of folded `f`: "visual studio code" → "vsc". */
export const initials = (f: string): string =>
  f
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)
    .map((w) => w[0])
    .join("");

/**
 * Match quality of folded query `q` in folded `f`: 6 exact, 5 prefix, 4 word-start, 3 word initials, 2 substring,
 * 1 in-order letters, 0 none. `ini` is `initials(f)`, passed in when precomputed.
 */
export function tier(q: string, f: string, ini = initials(f)): number {
  if (f === q) return 6;
  if (f.startsWith(q)) return 5;
  const at = f.indexOf(q);
  for (let i = at; i >= 0; i = f.indexOf(q, i + 1)) if (!WORD.test(f[i - 1])) return 4;
  if (q.length > 1 && ini.startsWith(q)) return 3;
  if (at >= 0) return 2;
  let j = 0;
  for (const ch of f) if (ch === q[j] && ++j === q.length) return 1;
  return 0;
}
