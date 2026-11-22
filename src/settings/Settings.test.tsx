import { expect, test, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SettingsPage, type View } from "./Settings";

const view: View = {
  settings: { hotkey: "Alt+Space", extraFolders: ["/Volumes/Tools"], ignoredApps: ["/Applications/Chess.app"], theme: "system", paletteStyle: "compact", dailyUpdateCheck: true },
  hotkeyError: "taken by another app",
  login: true,
  loginRefusal: null,
  version: "0.1.2",
  update: null,
};

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

test("Settings exposes lists, alerts and the hotkey's name to assistive tech", () => {
  const html = renderToStaticMarkup(<SettingsPage initial={view} />);
  // folder and app lists keep list semantics without bullets
  expect(html.match(/<ul class="paths" role="list">/g)).toHaveLength(2);
  expect(html).toContain("/Volumes/Tools");
  expect(html).toContain('aria-label="Remove /Applications/Chess.app"');
  // a broken hotkey is announced
  expect(html).toMatch(/<p class="error" role="alert">The hotkey is not working: taken by another app\./);
  // the button's name is "Hotkey" plus the chord
  expect(html).toContain('<span id="hotkey-label">Hotkey</span>');
  expect(html).toMatch(/<button type="button" id="hotkey" class="recorder" aria-labelledby="hotkey-label hotkey" aria-describedby="hotkey-hint">⌥Space<\/button>/);
  // the heading the page focuses once loaded can take the focus
  expect(html).toContain('<h2 id="general" tabindex="-1">General</h2>');
});
