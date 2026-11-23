import { expect, test, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { command, PaletteView, rowAt, total, type Style } from "./Palette";
import { paletteKey } from "./keys";
import { move, prepare, search, type Item } from "./rank";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string, s: string) => `${s}://localhost/${encodeURIComponent(p)}` }));

const app = (name: string): Item => ({ kind: "app", id: `/Applications/${name}.app`, name, altNames: [], path: `/Applications/${name}.app` });
const apps = ["Finder", "Firefox", "Final Cut Pro", "Safari"].map(app);
const rows = search("fi", prepare(apps), {});

/** Renders `style` after pressing `keys`, through the same key handler and selection logic the palette uses. */
function render(style: Style, keys: string[], update: string | null = null, cmd = false, list = rows) {
  let sel = 0;
  const actions = { hide() {}, move: (d: number) => (sel = move(sel, d, rows.length)), open() {}, openAt() {}, settings() {}, update() {} };
  for (const key of keys) paletteKey({ key, code: "", isComposing: false, keyCode: 0, metaKey: false, shiftKey: false, preventDefault() {} }, actions);
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
    renderToStaticMarkup(<PaletteView style="compact" query="2+3*4" rows={rows} result="14" sel={sel} cmd={cmd} error="" onQuery={() => {}} onOpen={() => {}} />);
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
  expect(rowAt(0, "14", rows)).toBe("14");
  expect(rowAt(1, "14", rows)).toBe(rows[0]);
  expect(rowAt(rows.length, "14", rows)).toBe(rows[rows.length - 1]);
  expect(rowAt(0, null, rows)).toBe(rows[0]);
  expect(total(rows, "14")).toBe(rows.length + 1);
  expect(total(rows, null)).toBe(rows.length);
  // Enter and ⌘Enter on the result copy, an app row launches (⌘Enter reveals), past the end does nothing
  expect(command(0, "14", rows, true)).toEqual(["copy_text", { text: "14" }]);
  expect(command(1, "14", rows, true)).toEqual(["run", { id: rows[0].id, reveal: true }]);
  expect(command(0, null, rows, false)).toEqual(["run", { id: rows[0].id, reveal: false }]);
  expect(command(rows.length + 1, "14", rows, false)).toBeNull();
  // the footer names the selected row's Enter action
  expect(first).toContain('<span class="keys" aria-hidden="true">↵ copy · ⌘1–9 open · esc close</span>');
  expect(second).toContain('<span class="keys" aria-hidden="true">↵ open · ⌘↵ reveal in Finder · ⌘1–9 open · esc close</span>');
});

test("a result shows the list even when no app matches", () => {
  const html = renderToStaticMarkup(<PaletteView style="roomy" query="1+1" rows={[]} result="2" sel={0} error="" onQuery={() => {}} onOpen={() => {}} />);
  expect(html.match(/role="option"/g)).toHaveLength(1);
  expect(html).toContain('aria-activedescendant="row-0"');
  expect(html).toContain('aria-label="Result 2, press Enter to copy"');
  expect(renderToStaticMarkup(<PaletteView style="roomy" query="zz" rows={[]} result={null} sel={0} error="" onQuery={() => {}} onOpen={() => {}} />)).not.toContain("listbox");
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
  expect(command(0, null, list, true)).toEqual(["run", { id: "pane:a", reveal: true }]);
  expect(command(1, "3", list, false)).toEqual(["run", { id: "pane:a", reveal: false }]);
  // ⌘Enter opens a section too: the footer does not offer "reveal in Finder" for one
  expect(html).toContain('<span class="keys" aria-hidden="true">↵ open · ⌘1–9 open · esc close</span>');
  const onApp = renderToStaticMarkup(<PaletteView {...props} sel={2} />);
  expect(onApp).toContain("⌘↵ reveal in Finder");
});
