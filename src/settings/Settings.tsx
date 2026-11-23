import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatChord, recorderStep } from "./chord";
import { addPath, removePath, serial } from "./form";

type Settings = {
  hotkey: string;
  extraFolders: string[];
  ignoredApps: string[];
  theme: "system" | "light" | "dark";
  paletteStyle: "compact" | "roomy";
  dailyUpdateCheck: boolean;
  systemCommands: boolean;
  calculator: boolean;
};
export type View = { settings: Settings; hotkeyError: string | null; login: boolean; loginRefusal: string | null; version: string; update: string | null };
type UpdateInfo = { version: string };

const CONFLICT_HINT = "If another app (Raycast, Alfred) uses this shortcut, both will respond; change one of them.";

/** Native controls laid out like macOS System Settings (plan §6). Every change is saved and applied at once; there is no Save button.
 * `initial` renders the page already loaded (a static render runs no effects, so it would never fetch). */
export function SettingsPage({ initial = null }: { initial?: View | null }) {
  const [view, setView] = useState<View | null>(initial);
  const [error, setError] = useState("");
  const [recording, setRecording] = useState(false);
  const [recMsg, setRecMsg] = useState("");
  const [checking, setChecking] = useState(false);
  const [checkMsg, setCheckMsg] = useState("");
  const latest = useRef<View | null>(initial);
  const first = useRef<HTMLHeadingElement>(null);
  const queue = useRef(serial()).current;

  const show = (v: View) => {
    latest.current = v;
    setView(v);
  };
  const reload = () => invoke<View>("get_settings").then(show);
  /** `patch` reads the settings as the last save left them, not as this render saw them (two quick removals). */
  const save = (patch: (s: Settings) => Partial<Settings>) =>
    queue(() => invoke<View>("set_settings", { patch: patch(latest.current!.settings) })).then(
      (v) => (show(v), setError("")),
      (e) => (setError(String(e)), reload()),
    );
  /** `on` is read in the handler: by the time the queue runs the patch, React has put the controlled checkbox back. */
  const saveChecked = (key: "systemCommands" | "calculator" | "dailyUpdateCheck", on: boolean) => save(() => ({ [key]: on }));
  const setLogin = (enabled: boolean) =>
    queue(() => invoke<boolean>("set_login", { enabled })).then(
      () => (setError(""), reload()),
      (e) => (setError(String(e)), reload()),
    );
  const pick = (kind: "folder" | "app", key: "extraFolders" | "ignoredApps") =>
    invoke<string | null>("pick_path", { kind }).then((p) => {
      if (p) save((s) => ({ [key]: addPath(s[key], p) }));
    });

  const checkNow = () => {
    setChecking(true);
    setCheckMsg("");
    invoke<UpdateInfo | null>("check_update")
      .then(
        (u) => setCheckMsg(u ? "" : `hop ${latest.current!.version} is up to date.`),
        (e) => setCheckMsg(`Could not check for updates: ${e}`),
      )
      .finally(() => (setChecking(false), reload()));
  };

  useEffect(() => {
    reload();
  }, []);
  // focus inside the page once it shows: with the focus on the web view itself, VoiceOver announces "HTML content"
  // and waits for VO-Shift-Down instead of reading the window
  const loaded = !!view;
  useEffect(() => {
    if (loaded) first.current?.focus();
  }, [loaded]);

  // while recording, the recorder owns the keys
  // shortcut: the saved hotkey stays registered while recording, so pressing it toggles the palette instead of being
  // recorded (it would be no change anyway); unregister it during recording if that confuses people
  // shortcut: the default macOS app menu's key equivalents (⌘Q, ⌘W, ⌘H) act before the page sees them, so they cannot
  // be recorded (they would make poor hotkeys anyway); a custom app menu without them would lose ⌘W and ⌘Q in Settings
  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const step = recorderStep(e);
      setRecording(step.recording);
      setRecMsg(step.msg ?? "");
      if (step.chord !== undefined) save(() => ({ hotkey: step.chord }));
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording]);

  // Esc closes the window; while recording, the recorder's capture listener takes Esc first, and a held Esc that
  // cancelled a recording must not then close the window on its repeats
  useEffect(() => {
    if (recording) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.repeat && !e.defaultPrevented) invoke("close_settings");
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [recording]);

  if (!view) return null;
  const s = view.settings;
  // WebKit draws `<input type="checkbox" switch>` as a macOS switch; React's typings do not know the attribute yet
  const toggle = { switch: "" };
  const list = (key: "extraFolders" | "ignoredApps", label: string, empty: string, add: string, kind: "folder" | "app") => (
    <div className="group" role="group" aria-label={label}>
      {s[key].length > 0 && (
        // WebKit drops list semantics from a list without bullets: role="list" keeps "list, 2 items" for VoiceOver
        <ul className="paths" role="list">
          {s[key].map((p) => (
            <li key={p} className="row">
              <span className="path" title={p}>
                {p}
              </span>
              <button type="button" onClick={() => save((cur) => ({ [key]: removePath(cur[key], p) }))} aria-label={`Remove ${p}`}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className={s[key].length ? "row end" : "row"}>
        {!s[key].length && <span className="note">{empty}</span>}
        <button type="button" onClick={() => pick(kind, key)}>
          {add}
        </button>
      </div>
    </div>
  );
  const radios = <K extends "theme" | "paletteStyle">(key: K, legend: string, options: [Settings[K], string][]) => (
    <div className="row">
      <span id={`${key}-label`}>{legend}</span>
      <span className="choices" role="radiogroup" aria-labelledby={`${key}-label`}>
        {options.map(([v, label]) => (
          <label key={v}>
            <input type="radio" name={key} checked={s[key] === v} onChange={() => save(() => ({ [key]: v }))} />
            {label}
          </label>
        ))}
      </span>
    </div>
  );

  return (
    <main>
      {error && (
        <p className="error banner" role="alert">
          {error}
        </p>
      )}

      <section aria-labelledby="general">
        <h2 id="general" ref={first} tabIndex={-1}>
          General
        </h2>
        <div className="group">
          <div className="row">
            <span id="hotkey-label">Hotkey</span>
            <button
              type="button"
              id="hotkey"
              className="recorder"
              aria-labelledby="hotkey-label hotkey"
              aria-describedby="hotkey-hint"
              onClick={() => (setRecording(true), setRecMsg(""))}
              onBlur={() => setRecording(false)}
            >
              {recording ? "Press a shortcut…" : formatChord(s.hotkey)}
            </button>
          </div>
          <p className="row note" id="hotkey-hint">
            {recording ? "Esc cancels, ⌫ removes the hotkey." : CONFLICT_HINT}
          </p>
          <label className="row">
            Start at login
            <input
              type="checkbox"
              {...toggle}
              checked={view.login}
              disabled={!!view.loginRefusal && !view.login}
              onChange={(e) => setLogin(e.target.checked)}
            />
          </label>
          {view.loginRefusal && <p className="row note">{view.loginRefusal}</p>}
        </div>
        {recMsg && (
          <p className="error" role="alert">
            {recMsg}
          </p>
        )}
        {view.hotkeyError && (
          <p className="error" role="alert">
            The hotkey is not working: {view.hotkeyError}. Open hop from the menu-bar icon, or pick another shortcut.
          </p>
        )}

        <h3>Extra folders to index</h3>
        {list("extraFolders", "Extra folders to index", "Only the standard Applications folders.", "Add folder…", "folder")}

        <h3>Ignored apps</h3>
        {list("ignoredApps", "Ignored apps", "No app is hidden from the palette.", "Add app…", "app")}
      </section>

      <section aria-labelledby="search">
        <h2 id="search">Search</h2>
        <div className="group">
          <label className="row">
            System commands
            <input type="checkbox" {...toggle} checked={s.systemCommands} onChange={(e) => saveChecked("systemCommands", e.target.checked)} />
          </label>
          <label className="row">
            Calculator
            <input type="checkbox" {...toggle} checked={s.calculator} onChange={(e) => saveChecked("calculator", e.target.checked)} />
          </label>
        </div>
      </section>

      <section aria-labelledby="appearance">
        <h2 id="appearance">Appearance</h2>
        <div className="group">
          {radios("theme", "Theme", [
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ])}
          {radios("paletteStyle", "Palette style", [
            ["compact", "Compact"],
            ["roomy", "Roomy"],
          ])}
        </div>
      </section>

      <section aria-labelledby="updates">
        <h2 id="updates">Updates</h2>
        <div className="group">
          <label className="row">
            Check every day
            <input type="checkbox" {...toggle} checked={s.dailyUpdateCheck} onChange={(e) => saveChecked("dailyUpdateCheck", e.target.checked)} />
          </label>
          <div className="row">
            <span>hop {view.version}</span>
            <button type="button" disabled={checking} onClick={checkNow}>
              {checking ? "Checking…" : "Check now"}
            </button>
          </div>
          {view.update && (
            <div className="row">
              <span>hop {view.update} is available.</span>
              <button type="button" onClick={() => invoke("open_whatsnew")}>
                What's new…
              </button>
            </div>
          )}
          <p className="row note" role="status">
            {checking ? "Checking for updates…" : checkMsg}
          </p>
        </div>
      </section>
    </main>
  );
}
