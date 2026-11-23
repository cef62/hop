import { expect, test } from "vitest";
import { LIMIT, boost, move, prepare, search, type Item } from "./rank";

const app = (name: string, altNames: string[] = []): Item => {
  const path = `/Applications/${altNames[0] ?? name}.app`;
  return { kind: "app", id: path, name, altNames, path };
};
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
  const all = Object.fromEntries(apps.map((a, i) => [a.id, i + 1]));
  expect(names("", all)).toHaveLength(6);
});

test("selection stays within the rows", () => {
  expect(move(0, -1, 5)).toBe(0);
  expect(move(4, 1, 5)).toBe(4);
  expect(move(2, 1, 5)).toBe(3);
  expect(move(3, 1, 0)).toBe(0);
});

// a timing budget: shared CI runners are several times slower and noisy, so it runs on the dev Mac only
test.skipIf(!!process.env.CI)("1000 apps searched under 5 ms per keystroke", () => {
  const word = () => Math.random().toString(36).slice(2, 2 + 3 + Math.floor(Math.random() * 6));
  const big = prepare(Array.from({ length: 1000 }, () => app(`${word()} ${word()}`, [word()])));
  search("warm", big, {}); // JIT warm-up, not part of the per-keystroke cost
  const t0 = performance.now();
  for (const q of ["a", "ab", "abc", "x", "vsc"]) search(q, big, {});
  expect((performance.now() - t0) / 5).toBeLessThan(5);
});

test("frecency is keyed by id: sections sharing the System Settings path rank apart", () => {
  const settings = "/System/Applications/System Settings.app";
  const pane = (id: string, name: string): Item => ({ kind: "pane", id: `pane:${id}`, name, altNames: [], path: settings });
  const list = prepare([app("Sound Studio"), pane("sound", "Sound"), pane("screen", "Screen Time")]);
  // same tier (prefix "s"): shorter name first without frecency; a used section rises above an unused app
  expect(search("s", list, {}).map((i) => i.name)).toEqual(["Sound", "Screen Time", "Sound Studio"]);
  expect(search("s", list, { "pane:screen": 3 }).map((i) => i.name)).toEqual(["Screen Time", "Sound", "Sound Studio"]);
  // a count under the shared path lifts no section
  expect(search("s", list, { [settings]: 9 }).map((i) => i.name)).toEqual(["Sound", "Screen Time", "Sound Studio"]);
  expect(search("", list, { "pane:screen": 3 }).map((i) => i.id)).toEqual(["pane:screen"]);
});

test("caps results at LIMIT", () => {
  const many = prepare(Array.from({ length: 50 }, (_, i) => app(`App ${i}`)));
  expect(search("app", many, {})).toHaveLength(LIMIT);
});

test("commands never join the empty-query list, however often used, but still rank when typed", () => {
  const cmd = (id: string, name: string, confirm?: string): Item => ({ kind: "command", id, name, altNames: [], confirm });
  const list = prepare([app("Safari"), app("Mail"), cmd("cmd:sleep", "Sleep"), cmd("cmd:empty-trash", "Empty Trash", "Press Enter again to empty the Trash")]);
  const fr = { "/Applications/Safari.app": 0.8, "cmd:sleep": 1, "cmd:empty-trash": 0.5 };
  // the habit "hotkey, Enter" on an empty palette must never sleep the Mac
  expect(search("", list, fr).map((r) => r.id)).toEqual(["/Applications/Safari.app"]);
  expect(search("sle", list, fr)[0].id).toBe("cmd:sleep");
});
