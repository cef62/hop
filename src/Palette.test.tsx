import { expect, test, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { PaletteView, type Style } from "./Palette";
import { paletteKey } from "./keys";
import { move, prepare, search, type App } from "./rank";

vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string, s: string) => `${s}://localhost/${encodeURIComponent(p)}` }));

const apps: App[] = ["Finder", "Firefox", "Final Cut Pro", "Safari"].map((name) => ({ name, altNames: [], path: `/Applications/${name}.app` }));
const rows = search("fi", prepare(apps), {});

/** Renders `style` after pressing `keys`, through the same key handler and selection logic the palette uses. */
function render(style: Style, keys: string[], update: string | null = null) {
  let sel = 0;
  const actions = { hide() {}, move: (d: number) => (sel = move(sel, d, rows.length)), open() {}, settings() {}, update() {} };
  for (const key of keys) paletteKey({ key, isComposing: false, keyCode: 0, metaKey: false, preventDefault() {} }, actions);
  const html = renderToStaticMarkup(<PaletteView style={style} query="fi" rows={rows} sel={sel} error="" update={update} onQuery={() => {}} onOpen={() => {}} />);
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
