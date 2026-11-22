/** Longer queries are never maths someone types into a launcher, and they bound the parser's recursion. */
const MAX = 200;

/**
 * The query as a maths expression: `+ - * / % ^`, parentheses, unary minus and plus, `×` and `÷`. Null unless the whole
 * query parses and has a binary operator, so a bare number ("2024", "-5") stays an app search, and for a non-finite
 * result (÷0, overflow). Rounded to 12 significant digits; huge and tiny results show in exponent form ("1e+21").
 * shortcut: `.` is the only decimal separator (no "1,5"), add a locale-aware tokenizer if users ask.
 */
export function calc(query: string): string | null {
  if (query.length > MAX) return null;
  const t = query.replace(/[×÷]/g, (c) => (c === "×" ? "*" : "/")).match(/\d+(?:\.\d+)?|\.\d+|\S/g) ?? [];
  let i = 0;
  let ops = 0;
  let bad = false;
  const eat = (...s: string[]) => (s.includes(t[i]) ? t[i++] : undefined);

  function expr(): number {
    let v = term();
    for (let o; (o = eat("+", "-")); ops++) v = o === "+" ? v + term() : v - term();
    return v;
  }
  function term(): number {
    let v = unary();
    for (let o; (o = eat("*", "/", "%")); ops++) v = o === "*" ? v * unary() : o === "/" ? v / unary() : v % unary();
    return v;
  }
  // unary minus binds looser than ^ (-2^2 = -4), but ^ takes a signed exponent (2^-1)
  function unary(): number {
    if (eat("-")) return -unary();
    if (eat("+")) return unary();
    const b = atom();
    return eat("^") ? (ops++, b ** unary()) : b;
  }
  function atom(): number {
    if (eat("(")) {
      const v = expr();
      if (!eat(")")) bad = true;
      return v;
    }
    if (/^\.?\d/.test(t[i] ?? "")) return Number(t[i++]);
    bad = true;
    return NaN;
  }

  const v = expr();
  if (bad || i < t.length || !ops || !Number.isFinite(v)) return null;
  return String(Number(v.toPrecision(12))); // String(-0) is "0"
}
