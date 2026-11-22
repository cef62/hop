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
};
type View = { settings: Settings; hotkeyError: string | null; login: boolean; loginRefusal: string | null; version: string };

const CONFLICT_HINT = "If another app (Raycast, Alfred) uses this shortcut, both will respond; change one of them.";

/** Plain controls until M4 brings Sakuga. Every change is saved and applied at once; there is no Save button. */
export function SettingsPage() {
  const [view, setView] = useState<View | null>(null);
  const [error, setError] = useState("");
  const [recording, setRecording] = useState(false);
  const [recMsg, setRecMsg] = useState("");
  const latest = useRef<View | null>(null);
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
  const setLogin = (enabled: boolean) =>
    queue(() => invoke<boolean>("set_login", { enabled })).then(
      () => (setError(""), reload()),
      (e) => (setError(String(e)), reload()),
    );
  const pick = (kind: "folder" | "app", key: "extraFolders" | "ignoredApps") =>
    invoke<string | null>("pick_path", { kind }).then((p) => {
      if (p) save((s) => ({ [key]: addPath(s[key], p) }));
    });

  useEffect(() => {
    reload();
  }, []);

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
  const list = (key: "extraFolders" | "ignoredApps", empty: string) =>
    s[key].length ? (
      <ul className="paths">
        {s[key].map((p) => (
          <li key={p}>
            <span className="path" title={p}>
              {p}
            </span>
            <button type="button" onClick={() => save((cur) => ({ [key]: removePath(cur[key], p) }))} aria-label={`Remove ${p}`}>
              Remove
            </button>
          </li>
        ))}
      </ul>
    ) : (
      <p className="note">{empty}</p>
    );
  const radios = <K extends "theme" | "paletteStyle">(key: K, options: [Settings[K], string][]) =>
    options.map(([v, label]) => (
      <label key={v} className="radio">
        <input type="radio" name={key} checked={s[key] === v} onChange={() => save(() => ({ [key]: v }))} />
        {label}
      </label>
    ));

  return (
    <main>
      {error && (
        <p className="error banner" role="alert">
          {error}
        </p>
      )}

      <section aria-labelledby="general">
        <h2 id="general">General</h2>
        <div className="field">
          <span className="label" id="hotkey-label">
            Hotkey
          </span>
          <button
            type="button"
            className="recorder"
            aria-labelledby="hotkey-label"
            aria-describedby="hotkey-hint"
            onClick={() => (setRecording(true), setRecMsg(""))}
            onBlur={() => setRecording(false)}
          >
            {recording ? "Press a shortcut…" : formatChord(s.hotkey)}
          </button>
        </div>
        <p className="note" id="hotkey-hint">
          {recording ? "Esc cancels, ⌫ removes the hotkey." : CONFLICT_HINT}
        </p>
        {recMsg && <p className="error">{recMsg}</p>}
        {view.hotkeyError && (
          <p className="error" role="alert">
            The hotkey is not working: {view.hotkeyError}. Open hop from the menu-bar icon, or pick another shortcut.
          </p>
        )}

        <label className="field">
          <input type="checkbox" checked={view.login} disabled={!!view.loginRefusal && !view.login} onChange={(e) => setLogin(e.target.checked)} />
          Start at login
        </label>
        {view.loginRefusal && <p className="note">{view.loginRefusal}</p>}

        <h3>Extra folders to index</h3>
        {list("extraFolders", "Only the standard Applications folders.")}
        <button type="button" onClick={() => pick("folder", "extraFolders")}>
          Add folder…
        </button>

        <h3>Ignored apps</h3>
        {list("ignoredApps", "No app is hidden from the palette.")}
        <button type="button" onClick={() => pick("app", "ignoredApps")}>
          Add app…
        </button>
      </section>

      <section aria-labelledby="appearance">
        <h2 id="appearance">Appearance</h2>
        <fieldset>
          <legend>Theme</legend>
          {radios("theme", [
            ["system", "System"],
            ["light", "Light"],
            ["dark", "Dark"],
          ])}
        </fieldset>
        <fieldset>
          <legend>Palette style</legend>
          {radios("paletteStyle", [
            ["compact", "Compact"],
            ["roomy", "Roomy"],
          ])}
        </fieldset>
      </section>

      <section aria-labelledby="updates">
        <h2 id="updates">Updates</h2>
        <button type="button" disabled>
          Check for updates now
        </button>
        <label className="field">
          <input type="checkbox" checked={s.dailyUpdateCheck} onChange={(e) => save(() => ({ dailyUpdateCheck: e.target.checked }))} />
          Check every day
        </label>
        <p className="note">Update checks arrive in a later version. This is hop {view.version}.</p>
      </section>
    </main>
  );
}
