import { expect, test, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { command, footer, PaletteView, type Style } from "./Palette";
import { leads, rowAt, total, type Entry, type Lead } from "./leads";
import { live } from "./confirm";
import { paletteKey } from "./keys";
import { move, prepare, search, type Item } from "./rank";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string, s: string) => `${s}://localhost/${encodeURIComponent(p)}` }));

const app = (name: string): Item => ({ kind: "app", id: `/Applications/${name}.app`, name, altNames: [], path: `/Applications/${name}.app` });
const apps = ["Finder", "Firefox", "Final Cut Pro", "Safari"].map(app);
const rows = search("fi", prepare(apps), {});
const answer = (text: string): Lead => ({ kind: "result", text });

/** Renders `style` after pressing `keys`, through the same key handler and selection logic the palette uses. */
function render(style: Style, keys: string[], update: string | null = null, cmd = false, list = rows) {
  let sel = 0;
  const actions = { hide() {}, move: (d: number) => (sel = move(sel, d, rows.length)), send() {}, settings() {}, update() {} };
  for (const key of keys) paletteKey({ key, code: "", isComposing: false, keyCode: 0, metaKey: false, shiftKey: false, repeat: false, preventDefault() {} }, actions);
  const html = renderToStaticMarkup(<PaletteView style={style} query="fi" rows={list} cmd={cmd} sel={sel} error="" update={update} onQuery={() => {}} onOpen={() => {}} />);
  return { sel, html: html.replace(`data-style="${style}"`, "") };
}

test("both styles render the same rows and move the selection the same way", () => {
  for (const keys of [[], ["ArrowDown"], ["ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown", "ArrowDown"], ["ArrowDown", "ArrowUp", "ArrowUp"]]) {
    expect(render("roomy", keys)).toEqual(render("compact", keys));
  }
  const { html, sel } = render("compact", ["ArrowDown"]);
  expect(sel).toBe(1);
  expect(html.match(/role="option"/g)).toHaveLength(rows.length);
  expect(html).toContain('id="row-1" class="row" role="option" aria-selected="true"');
  expect(html).toContain('src="icon://localhost/%2FApplications%2FFirefox.app"');
});

test("the update pill shows only with a pending update, the same in both styles", () => {
  expect(render("compact", []).html).not.toContain("pill");
  const roomy = render("roomy", [], "0.2.0");
  expect(roomy).toEqual(render("compact", [], "0.2.0"));
  expect(roomy.html).toContain("data-update");
  expect(roomy.html).toMatch(/<button type="button" class="pill" tabindex="-1" aria-keyshortcuts="Meta\+U">Update to 0.2.0 /);
});

test("⌘N hints show only while ⌘ is held, on the first 9 rows, the same in both styles", () => {
  const many = Array.from({ length: 11 }, (_, i) => app(`App ${i}`));
  const held = render("compact", [], null, true, many);
  expect(held).toEqual(render("roomy", [], null, true, many));
  expect(held.html.match(/<span class="hint" aria-hidden="true">⌘\d<\/span>/g)).toHaveLength(9);
  expect(held.html).toContain('aria-hidden="true">⌘9</span>');
  expect(held.html).not.toContain("⌘10");
  expect(held.html).not.toContain("hint-label"); // the "↵ open" hint (the footer is separate)
  const free = render("compact", [], null, false, many);
  expect(free.html).not.toMatch(/⌘\d</);
  expect(free.html).toContain("hint-label");
  for (const html of [held.html, free.html]) {
    expect(html.match(/aria-keyshortcuts=/g)).toHaveLength(9);
    expect(html).toContain('aria-keyshortcuts="Meta+1"');
    expect(html).toContain('aria-keyshortcuts="Meta+9"');
  }
});

test("a calculator result is row 0: it copies, the apps follow it, and ⌘N and the hints count it", () => {
  const view = (sel: number, cmd = false) =>
    renderToStaticMarkup(<PaletteView style="compact" query="2+3*4" rows={rows} leads={[answer("14")]} sel={sel} cmd={cmd} error="" onQuery={() => {}} onOpen={() => {}} />);
  const first = view(0);
  expect(first.match(/role="option"/g)).toHaveLength(rows.length + 1);
  expect(first).toContain('aria-activedescendant="row-0"');
  expect(first).toMatch(/<li id="row-0" class="row" role="option" aria-selected="true" aria-keyshortcuts="Meta\+1" aria-label="Result 14, press Enter to copy"><span class="icon calc" aria-hidden="true">=<\/span><span class="name">= 14<\/span><span class="hint" aria-hidden="true">↵<span class="hint-label"> copy<\/span>/);
  expect(first.indexOf('id="row-0"')).toBeLessThan(first.indexOf("Finder"));
  expect(first).toMatch(/id="row-1" class="row" role="option" aria-selected="false" aria-keyshortcuts="Meta\+2"><span class="icon"><img src="icon:\/\/localhost\/%2FApplications%2FFinder.app"/);
  expect(first).not.toContain("> open<");
  expect(first.match(/<img/g)).toHaveLength(rows.length); // the result row requests no icon
  const second = view(1);
  expect(second).toContain("> open<");
  expect(second).not.toContain("> copy<");
  const held = view(0, true);
  expect(held).toContain('aria-hidden="true">⌘1</span>');
  expect(held).toContain(`aria-hidden="true">⌘${rows.length + 1}</span>`);
  // the selection moves over the combined list, Enter and ⌘1 on row 0 reach the result, not an app
  expect(move(0, 1, rows.length + 1)).toBe(1);
  expect(move(rows.length - 1, 1, rows.length + 1)).toBe(rows.length); // the last app stays reachable
  expect(rowAt(0, [answer("14")], rows)).toEqual(answer("14"));
  expect(rowAt(1, [answer("14")], rows)).toBe(rows[0]);
  expect(rowAt(rows.length, [answer("14")], rows)).toBe(rows[rows.length - 1]);
  expect(rowAt(0, [], rows)).toBe(rows[0]);
  expect(total(rows, [answer("14")])).toBe(rows.length + 1);
  expect(total(rows, [])).toBe(rows.length);
  // Enter and ⌘Enter on the result copy, under the trimmed query; an app row launches (⌘Enter reveals); past the end does nothing
  const one = [answer("14")];
  expect(command(0, one, rows, true, " 2+3*4 ")).toEqual(["copy_text", { text: "14", expr: "2+3*4" }]);
  expect(command(0, one, rows, false, "2+3*4")).toEqual(["copy_text", { text: "14", expr: "2+3*4" }]);
  expect(command(1, one, rows, true, "2+3*4")).toEqual(["run", { id: rows[0].id, reveal: true }]);
  expect(command(0, [], rows, false, "fi")).toEqual(["run", { id: rows[0].id, reveal: false }]);
  expect(command(rows.length + 1, one, rows, false, "2+3*4")).toBeNull();
  // the footer names the selected row's Enter action
  expect(first).toContain('<span class="keys" aria-hidden="true">↵ copy · ⌘1–9 open · esc close</span>');
  expect(second).toContain('<span class="keys" aria-hidden="true">↵ open · ⌘↵ reveal in Finder · ⌘1–9 open · esc close</span>');
});

test("a result shows the list even when no app matches", () => {
  const html = renderToStaticMarkup(<PaletteView style="roomy" query="1+1" rows={[]} leads={[answer("2")]} sel={0} error="" onQuery={() => {}} onOpen={() => {}} />);
  expect(html.match(/role="option"/g)).toHaveLength(1);
  expect(html).toContain('aria-activedescendant="row-0"');
  expect(html).toContain('aria-label="Result 2, press Enter to copy"');
  expect(renderToStaticMarkup(<PaletteView style="roomy" query="zz" rows={[]} sel={0} error="" onQuery={() => {}} onOpen={() => {}} />)).not.toContain("listbox");
});

/** The React keys of the rows `PaletteView` returns (it uses no hooks, so it can be called as a function). */
function rowKeys(node: unknown): string[] {
  if (Array.isArray(node)) return node.flatMap(rowKeys);
  if (!node || typeof node !== "object" || !("props" in node)) return [];
  const el = node as React.ReactElement<{ children?: unknown }>;
  return el.type === "li" ? [String(el.key)] : rowKeys(el.props.children);
}

const settings = "/System/Applications/System Settings.app";
const pane = (id: string, name: string): Item => ({ kind: "pane", id, name, altNames: [], path: settings });

test("sections share the System Settings icon but render keyed by id, tagged, opened with run", () => {
  const list = [pane("pane:a", "Sound"), pane("pane:b", "Screen Time"), app("System Settings")];
  list[2] = { ...list[2], id: settings, path: settings };
  const props = { style: "compact" as Style, query: "s", rows: list, sel: 0, error: "", onQuery() {}, onOpen() {} };
  expect(rowKeys(PaletteView(props))).toEqual(["pane:a", "pane:b", settings]);
  const html = renderToStaticMarkup(<PaletteView {...props} />);
  // the field searches more than apps now, and VoiceOver reads its label
  expect(html).toContain('placeholder="Search apps and settings" aria-label="Search apps and settings"');
  expect(html.match(/<span class="tag">System Settings<\/span>/g)).toHaveLength(2); // the app row has no tag
  expect(html.match(/src="icon:\/\/localhost\/%2FSystem%2FApplications%2FSystem%20Settings.app"/g)).toHaveLength(3);
  expect(command(0, [], list, true, "s")).toEqual(["run", { id: "pane:a", reveal: true }]);
  expect(command(1, [answer("3")], list, false, "1+2")).toEqual(["run", { id: "pane:a", reveal: false }]);
  // ⌘Enter opens a section too: the footer does not offer "reveal in Finder" for one
  expect(html).toContain('<span class="keys" aria-hidden="true">↵ open · ⌘1–9 open · esc close</span>');
  const onApp = renderToStaticMarkup(<PaletteView {...props} sel={2} />);
  expect(onApp).toContain("⌘↵ reveal in Finder");
});

const cmd = (id: string, name: string, confirm?: string): Item => ({ kind: "command", id: `cmd:${id}`, name, altNames: [], confirm });
const commands = [cmd("restart", "Restart", "Press Enter again to restart"), cmd("lock", "Lock Screen"), app("Safari")];

test("an armed row reads its confirm copy, to the eye and to VoiceOver", () => {
  const view = (armed: string | null, sel = 0) =>
    renderToStaticMarkup(<PaletteView style="roomy" query="re" rows={commands} armed={armed} sel={sel} error="" onQuery={() => {}} onOpen={() => {}} />);
  const armed = view("cmd:restart");
  expect(armed).toMatch(
    /<li id="row-0" class="row" role="option" aria-selected="true" aria-keyshortcuts="Meta\+1" aria-label="Press Enter again to restart">.*<span class="name">Press Enter again to restart<\/span><span class="hint" aria-hidden="true">↵<span class="hint-label"> confirm<\/span>/,
  );
  expect(armed).toContain('<p class="status" role="status">Press Enter again to restart</p>');
  expect(armed).toContain('<span class="keys" aria-hidden="true">↵ confirm · esc close</span>');
  // not armed: the name, a "run" hint, and an empty live region that is already there to announce the next arming
  const idle = view(null);
  expect(idle).not.toContain("Press Enter again");
  expect(idle).toContain('<span class="name">Restart</span><span class="hint" aria-hidden="true">↵<span class="hint-label"> run<');
  expect(idle).toContain('<p class="status" role="status"></p>');
  expect(idle).toContain('<span class="keys" aria-hidden="true">↵ run · ⌘1–9 open · esc close</span>');
  // an armed id that left the list (the switch turned commands off) shows nothing
  expect(view("cmd:gone")).toEqual(idle);
  // the palette passes only a live arming (`live`: the armed row is the selected one), so the view never shows a prompt
  // on one row while Enter acts on another
  expect(view(live({ armed: { id: "cmd:restart", q: "re" }, sel: 2 }, (i) => commands[i], "re"), 2)).toEqual(view(null, 2));
});

test("command rows draw a glyph, request no icon, and run by id", () => {
  const html = renderToStaticMarkup(<PaletteView style="compact" query="l" rows={commands} sel={1} error="" onQuery={() => {}} onOpen={() => {}} />);
  expect(html.match(/<span class="icon glyph" aria-hidden="true"><svg viewBox="0 0 24 24">/g)).toHaveLength(2);
  expect(html.match(/<img/g)).toHaveLength(1); // Safari only
  expect(html).not.toContain("icon://localhost/\""); // no request for an empty path
  expect(command(1, [], commands, false, "l")).toEqual(["run", { id: "cmd:lock", reveal: false }]);
});

const old: Entry = { expr: "12*7+3", result: "87" };
const history: Entry[] = [old, { expr: "2^64", result: "1.84467440737e+19" }, { expr: "9/4", result: "2.25" }];

test("a sum shows its answer, then history rows `expr = result` with the hint ↵ edit; items follow", () => {
  const lead = leads("2+2", "4", history, true);
  const view = (sel: number, cmd = false, l = lead) =>
    renderToStaticMarkup(<PaletteView style="compact" query="2+2" rows={rows} leads={l} sel={sel} cmd={cmd} error="" onQuery={() => {}} onOpen={() => {}} />);
  const html = view(1);
  expect(html.match(/role="option"/g)).toHaveLength(1 + history.length + rows.length);
  expect(html).toContain('<span class="name">12*7+3 = 87</span><span class="hint" aria-hidden="true">↵<span class="hint-label"> edit</span>');
  expect(html).toContain('<span class="name">2^64 = 1.84467440737e+19</span>');
  // under a complete sum there is no place for an insert: neither the label nor the footer offers it
  expect(html).toContain('aria-label="12*7+3 equals 87, press Enter to edit it"');
  expect(html).not.toContain("insert");
  // order: live row, history, items
  expect(html.indexOf("= 4<")).toBeLessThan(html.indexOf("12*7+3 = 87"));
  expect(html.indexOf("9/4 = 2.25")).toBeLessThan(html.indexOf("Finder"));
  // only the selected row has the hint; the others none
  expect(html.match(/hint-label/g)).toHaveLength(1);
  // history rows draw a glyph, request no icon
  expect(html.match(/<img/g)).toHaveLength(rows.length);
  expect(html).toContain('<span class="keys" aria-hidden="true">↵ edit · ⌘1–9 pick · esc close</span>');
  // ⌘ held: ⌘N over the combined rows, ⌘1 on the live row, ⌘2 on the first history row
  const held = view(1, true);
  expect(held).toMatch(/id="row-0"[^>]*>.*?⌘1<\/span>/);
  expect(held).toMatch(/id="row-1"[^>]*aria-keyshortcuts="Meta\+2".*?12\*7\+3 = 87.*?⌘2<\/span>/);
  expect(held).toContain(`⌘${1 + history.length + 1}</span>`); // the first item
  // the live result selected: "↵ copy"
  expect(view(0)).toContain("↵ copy · ⌘1–9 open · esc close");
  expect(view(0)).toContain("<span class=\"hint-label\"> copy</span>");
  // an item selected: its own footer
  expect(view(1 + history.length)).toContain("↵ open · ⌘↵ reveal in Finder");
});

test("an open sum lists history rows alone, and `=` offers the insert", () => {
  const view = (q: string, lead: Lead[]) => renderToStaticMarkup(<PaletteView style="compact" query={q} rows={[]} leads={lead} sel={0} error="" onQuery={() => {}} onOpen={() => {}} />);
  const open = view("1+", leads("1+", null, history, true));
  expect(open.match(/role="option"/g)).toHaveLength(3);
  expect(open).not.toContain("Result ");
  expect(open).toContain("↵ edit · ⌘↵ insert");
  expect(open).toContain("Command Enter to insert the result");
  expect(view("=", leads("=", null, history, true))).toContain("⌘↵ insert");
});

test("= lists the history alone, the first row selected; no entries: no list", () => {
  const lead = leads("=", null, history, true);
  const html = renderToStaticMarkup(<PaletteView style="compact" query="=" rows={[]} leads={lead} sel={0} error="" onQuery={() => {}} onOpen={() => {}} />);
  expect(html.match(/role="option"/g)).toHaveLength(3);
  expect(html).toContain('id="row-0" class="row" role="option" aria-selected="true"');
  expect(html).not.toContain("Result ");
  expect(html).toContain('aria-activedescendant="row-0"');
  expect(renderToStaticMarkup(<PaletteView style="compact" query="=" rows={[]} leads={leads("=", null, [], true)} sel={0} error="" onQuery={() => {}} onOpen={() => {}} />)).not.toContain("listbox");
});

test("rows: 1 result + 5 history + 3 items count, number and select in that order", () => {
  const many: Entry[] = Array.from({ length: 8 }, (_, i) => ({ expr: `${i}+1`, result: String(i + 1) }));
  const lead = leads("2+2", "4", many, true);
  const three = ["Finder", "Firefox", "Safari"].map(app);
  expect(total(three, lead)).toBe(9);
  expect(rowAt(0, lead, three)).toEqual(answer("4"));
  expect(rowAt(5, lead, three)).toEqual({ kind: "history", expr: "4+1", result: "5" });
  expect(rowAt(6, lead, three)).toBe(three[0]);
  expect(rowAt(8, lead, three)).toBe(three[2]);
  expect(rowAt(9, lead, three)).toBeUndefined();
  // history rows never run a command: the reducer edits the query
  for (let i = 1; i <= 5; i++) expect(command(i, lead, three, false, "2+2")).toBeNull();
  expect(command(6, lead, three, true, "2+2")).toEqual(["run", { id: three[0].id, reveal: true }]);
  const html = renderToStaticMarkup(<PaletteView style="compact" query="2+2" rows={three} leads={lead} sel={8} cmd error="" onQuery={() => {}} onOpen={() => {}} />);
  expect(html).toContain('aria-hidden="true">⌘9</span>');
  expect(html.match(/aria-keyshortcuts=/g)).toHaveLength(9);
  expect(html).toContain('id="row-8" class="row" role="option" aria-selected="true"');
});

test("history row keys are apart from item ids", () => {
  const lead = leads("2+2", "4", [{ expr: "pane:a", result: "1" }], true);
  const view = PaletteView({ style: "compact", query: "2+2", rows: [pane("pane:a", "Sound")], leads: lead, sel: 0, error: "", onQuery() {}, onOpen() {} });
  expect(rowKeys(view)).toEqual(["=", "history:pane:a", "pane:a"]);
});

test("the footer names the selected row's keys", () => {
  expect(footer(undefined, null)).toContain("↵ open");
  expect(footer(answer("1"), null)).toBe("↵ copy · ⌘1–9 open · esc close");
  const past: Lead = { kind: "history", expr: "1+1", result: "2" };
  expect(footer(past, null, "=")).toBe("↵ edit · ⌘↵ insert · ⌘1–9 pick · esc close");
  expect(footer(past, null, "3+")).toBe("↵ edit · ⌘↵ insert · ⌘1–9 pick · esc close");
  expect(footer(past, null, "3+4")).toBe("↵ edit · ⌘1–9 pick · esc close");
  expect(footer(past, null)).toContain("⌘↵ insert"); // no query: an empty one
});
