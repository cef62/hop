// @vitest-environment happy-dom
// The real Palette in a DOM, with only the Tauri bridge faked: the glue between DOM events and the pure modules.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { Palette } from "./Palette";
import type { Item } from "./rank";

const bridge = vi.hoisted(() => ({
  calls: [] as [string, Record<string, unknown> | undefined][],
  replies: {} as Record<string, unknown>,
  handlers: {} as Record<string, (e: { payload: unknown }) => void>,
}));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string, s: string) => `${s}://localhost/${encodeURIComponent(p)}`,
  invoke: (cmd: string, args?: Record<string, unknown>) => {
    bridge.calls.push([cmd, args]);
    const r = bridge.replies[cmd];
    return r instanceof Error ? Promise.reject(r) : Promise.resolve(r);
  },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, f: (e: { payload: unknown }) => void) => {
    bridge.handlers[name] = f;
    return Promise.resolve(() => delete bridge.handlers[name]);
  },
}));

const app = (name: string): Item => ({ kind: "app", id: `/Applications/${name}.app`, name, altNames: [], path: `/Applications/${name}.app` });
const restart: Item = { kind: "command", id: "cmd:restart", name: "Restart", altNames: [], confirm: "Press Enter again to restart" };
const items = [app("Safari"), app("7-Zip"), app("Mail"), app("Maps"), restart];
const look = { style: "compact", perf: false, update: null, calculator: true };

let root: Root;
let host: HTMLElement;
const runs = () => bridge.calls.filter(([c]) => c === "run" || c === "copy_text");
const called = (cmd: string) => bridge.calls.filter(([c]) => c === cmd);
const input = () => host.querySelector<HTMLInputElement>("input.search")!;
const rows = () => [...host.querySelectorAll("li.row")].map((r) => r.textContent!);
const selected = () => [...host.querySelectorAll("li.row")].findIndex((r) => r.getAttribute("aria-selected") === "true");
const settle = () => act(async () => void (await new Promise((r) => setTimeout(r, 0))));
const emit = (name: string, payload: unknown = look) => act(async () => void bridge.handlers[name]({ payload }));
const key = (type: "keydown" | "keyup", k: string, init: KeyboardEventInit = {}) => act(() => void window.dispatchEvent(new KeyboardEvent(type, { key: k, bubbles: true, cancelable: true, ...init })));
const down = (k: string, init: KeyboardEventInit = {}) => key("keydown", k, init);
const type = (v: string) =>
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input(), v);
    input().dispatchEvent(new Event("input", { bubbles: true }));
  });

async function mount(replies: Record<string, unknown> = {}) {
  bridge.replies = { list_apps: { generation: 1, items }, frecency: {}, picks: {}, calc_history: [], ...replies };
  host = document.body.appendChild(document.createElement("div"));
  root = createRoot(host);
  await act(async () => root.render(<Palette />));
  await settle();
}

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  bridge.calls = [];
});
afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

test("typing updates the rows", async () => {
  await mount();
  expect(rows()).toEqual([]);
  await type("saf");
  expect(rows()).toEqual(["Safari↵ open"]);
  await type("ma");
  expect(rows().map((r) => r.replace("↵ open", ""))).toEqual(["Mail", "Maps"]);
});

test("7- lists 7-Zip first and no history row; 1+ with no match lists history (#28)", async () => {
  await mount({ calc_history: [{ expr: "2*3", result: "6" }] });
  await emit("palette-shown");
  await type("7-");
  expect(rows()).toEqual(["7-Zip↵ open"]);
  await type("1+");
  expect(rows()).toEqual(["2*3 = 6↵ edit"]);
});

test("Enter runs the app with the folded pick key; ⌘Enter reveals", async () => {
  await mount();
  await type("Saf");
  await down("Enter");
  expect(runs()).toEqual([["run", { id: "/Applications/Safari.app", reveal: false, query: "saf" }]]);
  await down("Enter", { metaKey: true });
  expect(runs()[1]).toEqual(["run", { id: "/Applications/Safari.app", reveal: true, query: "saf" }]);
});

test("Enter on a calculator result copies it with the expression", async () => {
  await mount();
  await type("2+3");
  await down("Enter");
  expect(runs()).toEqual([["copy_text", { text: "5", expr: "2+3" }]]);
});

test("Restart: Enter arms, the second Enter runs once", async () => {
  await mount();
  await type("restart");
  await down("Enter");
  expect(rows()[0]).toContain("Press Enter again to restart");
  expect(runs()).toEqual([]);
  await down("Enter");
  expect(runs()).toEqual([["run", { id: "cmd:restart", reveal: false, query: "restart" }]]);
});

test("Restart: a held Enter, ⌘N, ⌘Enter, another key, a query change, a pointer down never confirm", async () => {
  await mount();
  await type("restart");
  const armed = async () => {
    await down("Enter");
    expect(rows()[0]).toContain("Press Enter again");
  };
  await armed();
  await down("Enter", { repeat: true });
  expect(rows()[0]).not.toContain("Press Enter again");
  expect(runs()).toEqual([]);
  await armed();
  await down("1", { code: "Digit1", metaKey: true });
  expect(rows()[0]).toContain("Press Enter again"); // ⌘N arms, never confirms
  expect(runs()).toEqual([]);
  await down("1", { code: "Digit1", metaKey: true, repeat: true });
  expect(rows()[0]).not.toContain("Press Enter again"); // a held ⌘N's repeats only disarm
  await down("1", { code: "Digit1", metaKey: true });
  await down("Enter", { metaKey: true });
  expect(rows()[0]).toContain("Press Enter again"); // ⌘Enter on an armed row only arms again
  expect(runs()).toEqual([]);
  await down("a");
  expect(rows()[0]).not.toContain("Press Enter again");
  await armed();
  await type("restar");
  expect(rows()[0]).not.toContain("Press Enter again");
  await type("restart");
  await armed();
  await act(() => void host.querySelector("li.row")!.dispatchEvent(new Event("pointerdown", { bubbles: true, composed: true })));
  expect(rows()[0]).not.toContain("Press Enter again");
  await down("Enter");
  expect(runs()).toEqual([]); // armed again by that Enter, not confirmed
  expect(called("hide_palette")).toEqual([]);
});

test("a click on a row arms a command and never confirms it", async () => {
  await mount();
  await type("restart");
  const click = () => act(() => void host.querySelector<HTMLElement>("li.row")!.click());
  await click();
  expect(rows()[0]).toContain("Press Enter again");
  await click();
  expect(runs()).toEqual([]);
  await type("saf");
  await click();
  expect(runs()).toEqual([["run", { id: "/Applications/Safari.app", reveal: false, query: "saf" }]]);
});

test("a pointer down outside the card hides the palette, inside it does not", async () => {
  await mount();
  await type("saf");
  await act(() => void host.querySelector("li.row")!.dispatchEvent(new Event("pointerdown", { bubbles: true, composed: true })));
  expect(called("hide_palette")).toEqual([]);
  await act(() => void document.body.dispatchEvent(new Event("pointerdown", { bubbles: true, composed: true })));
  expect(called("hide_palette")).toHaveLength(1);
});

test("palette-shown loads frecency, picks and history and focuses the input", async () => {
  await mount();
  input().blur();
  bridge.calls = [];
  bridge.replies.frecency = { "/Applications/Safari.app": 1 };
  bridge.replies.picks = { ma: { "/Applications/Maps.app": 1 } };
  bridge.replies.calc_history = [{ expr: "2*3", result: "6" }];
  await emit("palette-shown");
  await settle();
  expect(["frecency", "picks", "calc_history"].every((c) => called(c).length === 1)).toBe(true);
  expect(document.activeElement).toBe(input());
  expect(rows()).toEqual(["Safari↵ open"]); // the new frecency
  await type("ma");
  expect(rows()[0]).toContain("Maps"); // the new picks
  await type("=");
  expect(rows()).toEqual(["2*3 = 6↵ edit"]); // the new history
});

test("picks loaded at start change the order", async () => {
  await mount({ picks: { ma: { "/Applications/Maps.app": 1 } } });
  await type("ma");
  expect(rows().map((r) => r.replace("↵ open", ""))).toEqual(["Maps", "Mail"]);
});

test("palette-hidden resets the query, the selection and the ⌘ hints", async () => {
  await mount({ frecency: { "/Applications/Safari.app": 2, "/Applications/Mail.app": 1 } });
  await type("restart");
  await down("Enter");
  await type("ma");
  await down("ArrowDown");
  await down("Meta");
  expect(selected()).toBe(1);
  expect(host.querySelector(".hint")!.textContent).toBe("⌘1");
  await emit("palette-hidden");
  expect(input().value).toBe("");
  expect(rows().map((r) => r.replace("↵ open", ""))).toEqual(["Safari", "Mail"]);
  expect(selected()).toBe(0);
  expect(host.querySelector(".hint")!.textContent).toBe("↵ open");
  await type("restart");
  expect(rows()[0]).not.toContain("Press Enter again");
});

test("Enter on a history row replaces the query, refocuses the input, caret last", async () => {
  await mount({ calc_history: [{ expr: "2*3", result: "6" }] });
  await emit("palette-shown");
  await type("=");
  input().blur();
  await down("Enter");
  expect(input().value).toBe("2*3");
  expect(document.activeElement).toBe(input());
  expect([input().selectionStart, input().selectionEnd]).toEqual([3, 3]);
  expect(runs()).toEqual([]);
});

test("⌘Enter on a history row inserts its result after an operator, and does nothing after a number", async () => {
  await mount({ calc_history: [{ expr: "2*3", result: "6" }] });
  await emit("palette-shown");
  await type("1+");
  input().blur();
  await down("Enter", { metaKey: true });
  expect(input().value).toBe("1+6");
  expect(document.activeElement).toBe(input());
  expect(input().selectionStart).toBe(3);
  await type("12");
  await down("Enter", { metaKey: true });
  expect(input().value).toBe("12");
});

test("⌘ down shows the ⌘N hints, keyup and window blur clear them", async () => {
  await mount();
  await type("ma");
  const hints = () => [...host.querySelectorAll(".hint")].map((h) => h.textContent);
  expect(hints()).toEqual(["↵ open"]);
  await down("Meta");
  expect(hints()).toEqual(["⌘1", "⌘2"]);
  await key("keyup", "Meta");
  expect(hints()).toEqual(["↵ open"]);
  await down("Meta");
  await act(() => void window.dispatchEvent(new Event("blur")));
  expect(hints()).toEqual(["↵ open"]);
});

const message = () => host.querySelector("p.message")?.textContent ?? null;

test("palette-shown with calculator off hides the result row and never copies; the next show restores it", async () => {
  await mount();
  await emit("palette-shown", { ...look, calculator: false });
  await type("2+2");
  expect(rows()).toEqual([]);
  await down("Enter");
  expect(called("copy_text")).toEqual([]);
  await emit("palette-shown", { ...look, calculator: true });
  expect(rows()).toEqual(["== 4↵ copy"]);
  await down("Enter");
  expect(called("copy_text")).toHaveLength(1);
});

test("palette-hidden carries the calculator switch too", async () => {
  await mount();
  await emit("palette-hidden", { ...look, calculator: false });
  await type("2+2");
  expect(rows()).toEqual([]);
  await emit("palette-hidden", { ...look, calculator: true });
  await type("2+2");
  expect(rows()).toEqual(["== 4↵ copy"]);
});

test("a rejected run shows its message, typing clears it", async () => {
  await mount({ run: new Error("boom") });
  await type("saf");
  await down("Enter");
  await settle();
  expect(message()).toBe("Error: boom");
  await type("safa");
  expect(message()).toBeNull();
});

test("a pending update shows the pill and ⌘U opens What's new; with none ⌘U does nothing", async () => {
  await mount();
  await down("u", { metaKey: true });
  expect(called("open_whatsnew")).toEqual([]);
  expect(host.querySelector(".pill")).toBeNull();
  await emit("palette-shown", { ...look, update: "0.9.0" });
  expect(host.querySelector(".pill")!.textContent).toContain("Update to 0.9.0");
  await down("u", { metaKey: true });
  expect(called("open_whatsnew")).toHaveLength(1);
});

test("picks and calc_history rejecting leave the palette listing and ranking items", async () => {
  await mount({ picks: new Error("no picks"), calc_history: new Error("no history") });
  await emit("palette-shown");
  await settle();
  await type("ma");
  expect(rows().map((r) => r.replace("↵ open", ""))).toEqual(["Mail", "Maps"]);
  await type("=");
  expect(rows()).toEqual([]);
});
