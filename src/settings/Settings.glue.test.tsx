// @vitest-environment happy-dom
// The real Settings page in a DOM: a switch must save its own key with the new value.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, test, vi } from "vitest";
import { SettingsPage, type View } from "./Settings";

const bridge = vi.hoisted(() => ({ calls: [] as [string, Record<string, unknown> | undefined][], view: null as unknown }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: { patch?: object }) => {
    bridge.calls.push([cmd, args]);
    let v = bridge.view as View;
    if (cmd === "set_settings") bridge.view = v = { ...v, settings: { ...v.settings, ...args!.patch } };
    return Promise.resolve(v);
  },
}));

test("the System commands and Calculator switches save their own key with the new value", async () => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  bridge.view = {
    settings: { hotkey: "Alt+Space", extraFolders: [], ignoredApps: [], theme: "system", paletteStyle: "compact", dailyUpdateCheck: true, systemCommands: true, calculator: true },
    hotkeyError: null,
    login: false,
    loginRefusal: null,
    version: "0.1.2",
    update: null,
  } satisfies View;
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  await act(async () => root.render(<SettingsPage />));
  const flip = (label: string) =>
    act(async () => {
      [...host.querySelectorAll("label")].find((l) => l.textContent === label)!.click(); // the label activates its switch, as a user click does
    });
  const patches = () => bridge.calls.filter(([c]) => c === "set_settings").map(([, a]) => a);
  await flip("System commands");
  await flip("Calculator");
  await flip("System commands");
  expect(patches()).toEqual([{ patch: { systemCommands: false } }, { patch: { calculator: false } }, { patch: { systemCommands: true } }]);
  act(() => root.unmount());
});
