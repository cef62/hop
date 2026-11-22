import { expect, test } from "vitest";
import { boost, move, prepare, search, type App } from "./rank";

const app = (name: string, altNames: string[] = []): App => ({ name, altNames, path: `/Applications/${altNames[0] ?? name}.app` });
const apps = [
  app("Visual Studio Code"),
  app("Impostazioni di Sistema", ["System Settings"]),
  app("Finder"),
  app("FaceTime"),
  app("Firefox"),
  app("Safari"),
  app("Final Cut Pro"),
];
const items = prepare(apps);
const names = (q: string, fr = {}) => search(q, items, fr).map((a) => a.name);

test("finds by prefix, abbreviation, localized name and the English file name", () => {
  expect(names("vsc")[0]).toBe("Visual Studio Code");
  expect(names("impost")).toEqual(["Impostazioni di Sistema"]);
  expect(names("system set")).toEqual(["Impostazioni di Sistema"]);
  expect(names("IMPOSTAZIONÌ")).toEqual(["Impostazioni di Sistema"]);
  expect(names("zzz")).toEqual([]);
});

test("frecency orders within a tier but never lifts a partial match over a prefix", () => {
  expect(names("fi")).toEqual(["Finder", "Firefox", "Final Cut Pro", "Safari", "FaceTime"]); // prefixes (shorter first), then in-order letters
  const heavy = { "/Applications/Final Cut Pro.app": 500 };
  expect(names("fi", heavy)[0]).toBe("Final Cut Pro");
  // "safari" is a prefix of Safari; FaceTime only matches f-a in order. A huge count does not change that
  expect(names("saf", { "/Applications/FaceTime.app": 1e9 })).toEqual(["Safari"]);
  expect(names("fa", { "/Applications/Safari.app": 1e9 })).toEqual(["FaceTime", "Safari", "Final Cut Pro"]);
  expect(boost(1e12)).toBeLessThan(1);
});

test("empty query: the most used apps, at most 6, nothing without history", () => {
  expect(names("")).toEqual([]);
  expect(names("  ", { "/Applications/Safari.app": 2, "/Applications/Finder.app": 5, "/Applications/Gone.app": 9 })).toEqual(["Finder", "Safari"]);
  const all = Object.fromEntries(apps.map((a, i) => [a.path, i + 1]));
  expect(names("", all)).toHaveLength(6);
});

test("selection stays within the rows", () => {
  expect(move(0, -1, 5)).toBe(0);
  expect(move(4, 1, 5)).toBe(4);
  expect(move(2, 1, 5)).toBe(3);
  expect(move(3, 1, 0)).toBe(0);
});

test("1000 apps searched under 5 ms per keystroke", () => {
  const word = () => Math.random().toString(36).slice(2, 2 + 3 + Math.floor(Math.random() * 6));
  const big = prepare(Array.from({ length: 1000 }, () => app(`${word()} ${word()}`, [word()])));
  const t0 = performance.now();
  for (const q of ["a", "ab", "abc", "x", "vsc"]) search(q, big, {});
  expect((performance.now() - t0) / 5).toBeLessThan(5);
});
