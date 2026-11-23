import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";
import { LIMIT, PICK_KEY, PICK_FORGET, boost, move, pickKey, prepare, search, strong, type Item, type Picks } from "./rank";

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
  const keys = Array.from({ length: 500 }, () => word().slice(0, 3));
  const picks = Object.fromEntries(keys.map((k, n) => [k, { [big[n].item.id]: 1 }])); // 500 keys
  search("warm", big, {}, picks); // JIT warm-up, not part of the per-keystroke cost
  const t0 = performance.now();
  for (const q of ["a", "ab", "abc", "x", "vsc"]) search(q, big, {}, picks);
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

describe("learned picks", () => {
  const list = prepare([app("Messages"), app("Google Meet"), app("Memo"), app("Mail"), app("Meeting Notes")]);
  const picks: Picks = { me: { [apps0("Google Meet")]: 1 } };
  function apps0(n: string) {
    return `/Applications/${n}.app`;
  }
  const go = (q: string, p: Picks = picks, fr = {}) => search(q, list, fr, p).map((a) => a.name);

  test("a pick for 'me' comes first for me, mee, meet, but not for m or mes", () => {
    expect(go("me", {})[0]).not.toBe("Google Meet"); // premise: Meet is not first without the pick
    expect(go("me")[0]).toBe("Google Meet");
    expect(go("mee")[0]).toBe("Google Meet");
    expect(go("meet")[0]).toBe("Google Meet");
    expect(go("m")).toEqual(go("m", {})); // learned key longer than the query
    expect(go("mes")).toEqual(go("mes", {}));
  });

  test("a picked item that stops matching is not shown", () => {
    expect(go("mes")).not.toContain("Google Meet");
    expect(go("mx")).not.toContain("Google Meet");
  });

  test("the query is folded and trimmed like the key", () => {
    expect(go("  ME ")[0]).toBe("Google Meet");
  });

  test("scores of every key that prefixes the query add up, and the higher sum wins", () => {
    const p = { me: { [apps0("Messages")]: 1, [apps0("Google Meet")]: 1.5 }, mem: { [apps0("Memo")]: 2 } };
    expect(go("me", p)[0]).toBe("Google Meet");
    expect(go("mem", p)[0]).toBe("Memo");
    expect(go("me", { me: { [apps0("Messages")]: 1 }, mee: { [apps0("Google Meet")]: 5 } })[0]).toBe("Messages"); // mee is longer than me
  });

  test("a one-letter key applies, and scores under several prefix keys sum", () => {
    expect(go("me", { m: { [apps0("Google Meet")]: 1 } })[0]).toBe("Google Meet");
    // Meet: 1 under m + 1 under me = 2, which beats Meeting Notes' single 1.5 under mee
    const p: Picks = { m: { [apps0("Google Meet")]: 1 }, me: { [apps0("Google Meet")]: 1 }, mee: { [apps0("Meeting Notes")]: 1.5 } };
    expect(go("mee", p)[0]).toBe("Google Meet");
  });

  test("empty query ignores picks", () => {
    expect(search("", list, {}, { "": { [apps0("Mail")]: 9 } })).toEqual([]);
    expect(search("  ", list, {}, { "": { [apps0("Mail")]: 9 } })).toEqual([]);
  });

  test("a pick outranks usage, and the rest keep today's order", () => {
    const fr = { [apps0("Messages")]: 50 };
    expect(go("me", picks, fr)).toEqual(["Google Meet", ...go("me", {}, fr).filter((n) => n !== "Google Meet")]);
  });

  test("unknown ids, zero scores and scores under the forget floor are inert", () => {
    expect(go("me", { me: { "/Gone.app": 3, [apps0("Memo")]: 0 } })).toEqual(go("me", {})); // Memo matches "me"
    expect(go("me", { me: { [apps0("Google Meet")]: PICK_FORGET / 2 } })).toEqual(go("me", {})); // an 8-week-old pick
    expect(go("me", { me: { [apps0("Google Meet")]: PICK_FORGET } })[0]).toBe("Google Meet");
  });

  test("any pick above the floor beats every unpicked match, however tiny or weak the tier", () => {
    const l = prepare([app("Meetings"), app("Zoom Meeting")]);
    const r = (p: Picks) => search("meet", l, {}, p).map((a) => a.name);
    expect(r({})).toEqual(["Meetings", "Zoom Meeting"]);
    expect(r({ meet: { [apps0("Zoom Meeting")]: 0.02 } })).toEqual(["Zoom Meeting", "Meetings"]);
  });

  test("picked items go by pick score, not by tier: 30 vs 45 and 10 vs 11", () => {
    const m = apps0("Messages");
    const g = apps0("Google Meet"); // Messages is the prefix match, Meet only a word start
    for (const [a, b] of [[30, 45], [10, 11], [50, 100]]) {
      expect(go("me", { me: { [m]: a, [g]: b } }).slice(0, 2)).toEqual(["Google Meet", "Messages"]);
      expect(go("me", { me: { [m]: b, [g]: a } }).slice(0, 2)).toEqual(["Messages", "Google Meet"]);
    }
    // equal pick scores fall back to today's order
    expect(go("me", { me: { [m]: 2, [g]: 2 } }).slice(0, 2)).toEqual(["Messages", "Google Meet"]);
  });

  test("a pick never applies through a scattered letter match (tier 1)", () => {
    const l = prepare([app("Safari"), app("Spotify"), app("Slack"), { kind: "command", id: "cmd:sleep", name: "Sleep", altNames: [] }, app("System Settings"), app("Signal")]);
    const ids = (q: string, p: Picks) => search(q, l, {}, p).map((a) => a.name);
    const sleep: Picks = { s: { "cmd:sleep": 1 } };
    expect(ids("sp", sleep)).toEqual(ids("sp", {})); // s…p are scattered in "sleep"
    expect(ids("sp", sleep)[0]).toBe("Spotify");
    expect(ids("s", sleep)[0]).toBe("Sleep"); // where it was picked it stays first
    const settings: Picks = { s: { [apps0("System Settings")]: 5 } };
    expect(ids("sig", settings)[0]).toBe("Signal");
    // a contiguous substring (tier 2) keeps the pick: "st" sits inside "system", Steam is a prefix match
    const withSteam = prepare([app("Steam"), app("System Settings")]);
    expect(search("st", withSteam, {}, settings).map((a) => a.name)).toEqual(["System Settings", "Steam"]);
  });

  test("the cap counts code points: an emoji at the boundary is never cut in half", () => {
    const k = pickKey("a".repeat(PICK_KEY - 1) + "😀x");
    expect(k).toBe("a".repeat(PICK_KEY - 1) + "😀");
    expect(() => encodeURIComponent(k)).not.toThrow(); // a lone surrogate throws
    expect(Array.from(pickKey("😀".repeat(40)))).toHaveLength(PICK_KEY);
  });

  test("the key is the folded, trimmed query capped at PICK_KEY chars", () => {
    expect(pickKey("  Réjouïr ")).toBe("rejouir");
    expect(pickKey("x".repeat(PICK_KEY + 5))).toHaveLength(PICK_KEY);
    expect(pickKey("   ")).toBe("");
  });

  test("an old long key still applies to its own prefix query", () => {
    const long = "a".repeat(PICK_KEY);
    const l = prepare([app(long + "zz"), app(long + "b")]);
    expect(search(long + "zz", l, {}, { [long]: { [apps0(long + "b")]: 1 } }).map((a) => a.name)[0]).toBe(long + "zz"); // no match for b
    expect(search(long, l, {}, { [long]: { [apps0(long + "b")]: 1 } }).map((a) => a.name)[0]).toBe(long + "b");
  });
});

test("strong: only a prefix, word-start or exact match of a query that starts with a letter or digit", () => {
  const p = (...n: string[]) => prepare(n.map((x) => app(x)));
  expect(strong("7-", p("7-Zip"))).toBe(true);
  expect(strong("1-", p("1Password 7 - Password Manager"))).toBe(false);
  expect(strong("(", p("Microsoft Teams (work or school)"))).toBe(false);
  expect(strong("-", p("Wi\u2011Fi"))).toBe(false);
  expect(strong("-", p("1Password 7 - Password Manager"))).toBe(false);
  // a word start counts for a query that starts with a word character, an alt name too
  expect(strong("7-z", p("Foo 7-Zip"))).toBe(true);
  expect(strong("7-", prepare([app("Archiver", ["7-Zip"])]))).toBe(true);
  // a loose match is not strong
  expect(strong("7-", p("Foo7-Zip"))).toBe(false);
  expect(strong("7-", p())).toBe(false);
  expect(strong("vsc", p("Visual Studio Code"))).toBe(false); // initials, tier 3
  expect(strong("sual", p("Visual Studio Code"))).toBe(false); // substring, tier 2
});


describe("system command names in other languages", () => {
  // The fixture is the Rust table itself, so the two cannot drift.
  const rust = readFileSync(new URL("../src-tauri/src/commands.rs", import.meta.url), "utf8");
  const cmds: Item[] = [...rust.matchAll(/^\s+\("(cmd:[\w-]+)", "([^"]+)", &\[([^\]]*)\],/gm)].map((m) => ({
    kind: "command",
    id: m[1],
    name: m[2],
    altNames: [...m[3].matchAll(/"([^"]+)"/g)].map((a) => a[1]),
  }));
  const names = [
    "Calculator", "Calendar", "Mail", "Maps", "Messages", "Music", "Notes", "Photos", "Podcasts", "Preview", "Reminders", "Shortcuts",
    "Safari", "Stickies", "Stocks", "Steam", "Slack", "Spotify", "Standard Notes", "Stats", "Settings", "Sublime Text", "Reeder", "Replay",
    "Raycast", "Rider", "Netflix", "Notion", "Blender", "Bildschirmfoto", "Abstract", "Ableton Live", "Apple Music", "App Store", "Logic Pro",
    "Terminal", "Telegram", "PowerPoint", "Stopwatch", "Monitor de Actividad", "Moniteur d’activité", "Capture d’écran", "Temps d’écran",
    "Captura de pantalla", "Cattura Schermata", "Festplattenutility", "Promemoria", "Erinnerungen", "Rappels", "Réglages Système",
    "Systemeinstellungen",
  ];
  const all = prepare([...names.map((n) => app(n)), ...cmds]);
  const appsOnly = prepare(names.map((n) => app(n)));

  test("the table has the 6 commands and 34 alt names", () => {
    expect(cmds).toHaveLength(6);
    expect(cmds.flatMap((c) => c.altNames)).toHaveLength(5 + 5 + 6 + 6 + 7 + 5); // with the English alts
  });

  test.each(cmds.flatMap((c) => c.altNames.map((a) => [a, c.id] as const)))("%s finds %s", (name, id) => {
    expect(search(name, all, {})[0].id).toBe(id);
    expect(search(name.toUpperCase(), all, {})[0].id).toBe(id);
  });

  test("folded spellings still match", () => {
    expect(search("eteindre", all, {})[0].id).toBe("cmd:shutdown");
    expect(search("redemarrer", all, {})[0].id).toBe("cmd:restart");
    expect(search("verrouiller l’ecran", all, {})[0].id).toBe("cmd:lock");
    expect(search("sospendi", all, {})[0].id).toBe("cmd:sleep");
  });

  test("a command never beats an app that ties it", () => {
    for (const [q, app] of [["su", "Sublime Text"], ["activ", "Moniteur d’activité"], ["pan", "Captura de pantalla"], ["fe", "Festplattenutility"], ["pow", "PowerPoint"], ["log", "Logic Pro"]])
      expect(search(q, all, {})[0].name, q).toBe(app);
    expect(["Capture d’écran", "Temps d’écran"]).toContain(search("ecran", all, {})[0].name); // both apps tie: shorter name
    for (const [q, id] of [["riavvia", "cmd:restart"], ["lock", "cmd:lock"], ["sospendi", "cmd:sleep"]]) expect(search(q, all, {})[0].id).toBe(id);
  });

  test("no 2-letter prefix or word-start prefix (2-6 letters) of an app puts Lock Screen or Sleep first, and a first-word prefix never puts any command first", () => {
    for (const a of names)
      for (const [wi, w] of a.split(/[^\p{L}\p{N}]+/u).filter(Boolean).entries())
        for (let n = 2; n <= Math.min(6, w.length); n++) {
          const q = w.slice(0, n);
          if (!search(q, appsOnly, {}).length) continue;
          const top = search(q, all, {})[0];
          // a later word starts at tier 4; a command's own prefix (tier 5, confirmed by a second Enter) may outrank it
          if (wi > 0 && top.kind === "command" && top.id !== "cmd:lock" && top.id !== "cmd:sleep") continue;
          expect(top.kind, `${q} -> ${top.name}`).toBe("app");
        }
  });
});
