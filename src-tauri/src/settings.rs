//! Settings (plan §5) in `settings.json`, app config dir. Versioned; keys with a wrong type or value read as defaults,
//! one key at a time; keys hop does not know (a newer version's) are kept and written back. A file that is not UTF-8
//! JSON is kept as `settings.json.corrupt` and hop starts from defaults. Written atomically. Start at login itself is not
//! stored here: the LaunchAgent is the truth (`login.rs`), as in velo; `loginApplied` only records that the first run
//! turned it on.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const VERSION: u32 = 1;
pub const STYLES: [&str; 2] = ["compact", "roomy"];
const THEMES: [&str; 3] = ["system", "light", "dark"];
pub const LABEL: &str = "settings";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    /// global-shortcut syntax (`Alt+Space`, `Control+Shift+Super+KeyK`); empty: no hotkey
    pub hotkey: String,
    /// indexed like `/Applications`, two levels deep
    pub extra_folders: Vec<String>,
    /// app paths left out of the index
    pub ignored_apps: Vec<String>,
    pub theme: String,
    /// the M2 key, unchanged
    pub palette_style: String,
    /// the daily update check (update.rs); the toggle applies at the checker's next wake
    pub daily_update_check: bool,
    /// Lock Screen, Sleep, Restart… in the index; off leaves them out, so `run` refuses them too
    pub system_commands: bool,
    /// start at login was turned on by a first run (or must be left alone); never set from the window
    pub login_applied: bool,
    /// keys this version does not know, written back as they were
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: VERSION,
            hotkey: crate::hotkey::DEFAULT.into(),
            extra_folders: vec![],
            ignored_apps: vec![],
            theme: "system".into(),
            palette_style: "compact".into(),
            daily_update_check: true,
            system_commands: true,
            login_applied: false,
            other: Default::default(),
        }
    }
}

impl Settings {
    fn valid(&self) -> bool {
        let abs = |v: &[String]| v.iter().all(|p| Path::new(p).is_absolute());
        THEMES.contains(&self.theme.as_str())
            && STYLES.contains(&self.palette_style.as_str())
            && crate::hotkey::valid(&self.hotkey)
            && abs(&self.extra_folders)
            && !self.extra_folders.iter().any(|f| too_broad(f))
            && abs(&self.ignored_apps)
    }
}

/// A folder whose two-level scan and recursive watch would cover the whole disk or every user's files.
/// Compared by text only (it runs at startup, before the hotkey: no disk access that a stalled network mount could
/// block): `..` and `.` resolved, case-folded as APFS compares names, the data-volume firmlink prefix dropped.
/// shortcut: a symlink to `/` (e.g. `/Volumes/Macintosh HD`) passes; resolve it off the main thread if it ever matters.
pub fn too_broad(folder: &str) -> bool {
    let key = |p: &str| {
        let mut parts: Vec<String> = Vec::new();
        for c in Path::new(p).components() {
            match c {
                std::path::Component::ParentDir => drop(parts.pop()),
                std::path::Component::Normal(n) => parts.push(n.to_string_lossy().to_lowercase()),
                _ => {}
            }
        }
        let joined = parts.join("/");
        joined.strip_prefix("system/volumes/data").map(|r| r.trim_start_matches('/').to_string()).unwrap_or(joined)
    };
    let f = key(folder);
    let home = std::env::var("HOME").ok().map(|h| key(&h));
    ["", "users", "volumes"].contains(&f.as_str()) || home.is_some_and(|h| f == h)
}

/// What the window says when it sent a refused key.
fn refusal(bad: &[String]) -> String {
    if bad.iter().any(|k| k == "extraFolders") {
        "hop does not index /, /Users, /Volumes or your home folder: pick the folder that holds the apps.".into()
    } else {
        format!("Not a valid value for {}", bad.join(", "))
    }
}

/// Keys `merge` never takes: hop writes them itself.
const OWN: [&str; 2] = ["version", "loginApplied"];

fn known(k: &str) -> bool {
    serde_json::to_value(Settings::default()).expect("settings serialize").get(k).is_some()
}

/// `patch`'s keys over `cur`, each taken only when it has the right type and a valid value; returns the keys refused
/// (unknown keys, `version` and `loginApplied` are skipped, not refused).
pub fn merge(cur: &Settings, patch: &Value) -> (Settings, Vec<String>) {
    let mut out = cur.clone();
    let mut bad = vec![];
    for (k, v) in patch.as_object().into_iter().flatten() {
        if OWN.contains(&k.as_str()) || !known(k) {
            continue;
        }
        let mut json = serde_json::to_value(&out).expect("settings serialize");
        json[k] = v.clone();
        match serde_json::from_value::<Settings>(json) {
            Ok(s) if s.valid() => out = s,
            _ => bad.push(k.clone()),
        }
    }
    out.version = VERSION;
    (out, bad)
}

/// The settings, and whether start at login still has to be applied: no file, or a file without `loginApplied` (from M2,
/// or saved by a dev build that may not turn it on). A corrupt or unreadable file is no first run: hop starts from
/// defaults and leaves start at login alone.
pub fn load(path: &Path) -> (Settings, bool) {
    let hands_off = Settings { login_applied: true, ..Settings::default() };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Settings::default(), true),
        Err(e) => {
            // shortcut: a later save from Settings still replaces it; nothing at startup writes over it
            eprintln!("hop: cannot read {} ({e}); started from defaults", path.display());
            return (hands_off, false);
        }
    };
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(obj)) => {
            let (mut s, bad) = merge(&Settings::default(), &Value::Object(obj.clone()));
            if !bad.is_empty() {
                eprintln!("hop: {} has invalid {}; using the defaults for those", path.display(), bad.join(", "));
            }
            // only a missing key is "not yet applied": a hand-edited value of another type must not turn start at login back on
            s.login_applied = obj.get("loginApplied").is_some_and(|v| v.as_bool() != Some(false));
            s.other = obj.into_iter().filter(|(k, _)| !known(k)).collect();
            let first = !s.login_applied;
            (s, first)
        }
        _ => {
            // not UTF-8 JSON (`from_slice` checks both): kept for the owner to look at; the next save writes a fresh file
            let aside = path.with_extension("json.corrupt");
            match std::fs::write(&aside, &bytes) {
                Ok(()) => eprintln!("hop: {} is not valid; kept it as {} and started from defaults", path.display(), aside.display()),
                Err(e) => eprintln!("hop: {} is not valid and could not be kept aside ({e}); started from defaults", path.display()),
            }
            (hands_off, false)
        }
    }
}

/// Atomic: a crash mid-write leaves the old file, never half a file.
pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, path)
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("settings.json"))
}

/// Loads the settings into app state; true when start at login still has to be applied (see `load`).
pub fn setup(app: &AppHandle) -> bool {
    let (s, first) = file(app).map(|f| load(&f)).unwrap_or_default();
    apply_theme(app, &s.theme);
    app.manage(Mutex::new(s));
    first
}

pub fn current(app: &AppHandle) -> Settings {
    app.state::<Mutex<Settings>>().lock().unwrap().clone()
}

/// Records that the first run applied start at login, so later starts leave it to the user.
pub fn mark_login_applied(app: &AppHandle) {
    let s = {
        let state = app.state::<Mutex<Settings>>();
        let mut s = state.lock().unwrap();
        s.login_applied = true;
        s.clone()
    };
    if let Some(f) = file(app)
        && let Err(e) = save(&f, &s)
    {
        eprintln!("hop: could not save {} ({e})", f.display());
    }
}

pub fn palette_style(app: &AppHandle) -> &'static str {
    let s = app.state::<Mutex<Settings>>().lock().unwrap().palette_style.clone();
    STYLES.into_iter().find(|k| *k == s).unwrap_or(STYLES[0])
}

/// The app's appearance: every window, the palette's vibrancy and `prefers-color-scheme` in both pages follow it.
fn apply_theme(app: &AppHandle, theme: &str) {
    app.set_theme(match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    });
}

/// What the Settings window shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    settings: Settings,
    /// why the saved hotkey is not registered, if it is not
    hotkey_error: Option<String>,
    login: bool,
    /// why start at login cannot be turned on from this copy of hop
    login_refusal: Option<&'static str>,
    version: String,
    /// the version a check found, waiting to install
    update: Option<String>,
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> View {
    View {
        settings: current(&app),
        hotkey_error: crate::hotkey::error(),
        login: crate::login::enabled(&app),
        login_refusal: crate::login::refusal(),
        version: app.package_info().version.to_string(),
        update: crate::update::pending(&app),
    }
}

/// One save at a time, so two quick changes cannot interleave their hotkey swaps or file writes. Not the settings
/// mutex: registering a hotkey waits on the main thread, which takes that mutex when the palette shows.
static SAVING: Mutex<()> = Mutex::new(());

/// Applies and saves the keys in `patch` (the window sends only what changed). All or nothing: a refused key, a hotkey
/// that does not register, or a failed write leaves the old settings in place and live.
#[tauri::command(async)]
pub fn set_settings(app: AppHandle, patch: Value) -> Result<View, String> {
    let _one = SAVING.lock().unwrap();
    let old = current(&app);
    let (new, bad) = merge(&old, &patch);
    if !bad.is_empty() {
        return Err(refusal(&bad));
    }
    let hotkey = new.hotkey != old.hotkey;
    if hotkey {
        crate::hotkey::change(&app, &old.hotkey, &new.hotkey)?;
    }
    if let Some(f) = file(&app)
        && let Err(e) = save(&f, &new)
    {
        if hotkey {
            crate::hotkey::change(&app, &new.hotkey, &old.hotkey).ok();
        }
        eprintln!("hop: could not save {} ({e})", f.display());
        return Err(format!("Could not save the settings: {e}"));
    }
    *app.state::<Mutex<Settings>>().lock().unwrap() = new.clone();
    if new.theme != old.theme {
        apply_theme(&app, &new.theme);
    }
    if new.extra_folders != old.extra_folders || new.ignored_apps != old.ignored_apps || new.system_commands != old.system_commands {
        crate::apps::reload(&app, new.extra_folders != old.extra_folders);
    }
    Ok(get_settings(app))
}

/// A folder to index (`kind` "folder") or an app to ignore ("app"), from the system picker. Picked here rather than
/// through the dialog plugin's JS API, so the Settings window needs no dialog permission.
#[tauri::command(async)]
pub fn pick_path(app: AppHandle, kind: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let d = app.dialog().file().set_parent(&app.get_webview_window(LABEL)?);
    let picked = if kind == "app" { d.set_directory("/Applications").add_filter("Apps", &["app"]).blocking_pick_file() } else { d.blocking_pick_folder() };
    picked?.into_path().ok().map(|p| p.to_string_lossy().into_owned())
}

/// Opens the Settings window, or brings it to the front. The palette hides first (plan §4.1).
pub fn open(app: &AppHandle) {
    crate::panel::hide_for_window(app);
    #[cfg(target_os = "macos")]
    app.show().ok(); // undo the palette's app.hide(), or the window stays hidden
    if let Some(w) = app.get_webview_window(LABEL) {
        w.unminimize().ok();
        w.show().ok();
        w.set_focus().ok();
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title("hop Settings")
        .inner_size(520.0, 720.0) // no scrollbar with one folder, one ignored app and one note (login or update)
        .min_inner_size(420.0, 420.0)
        .minimizable(false)
        .maximizable(false)
        .center()
        .build();
    match built {
        Ok(w) => {
            w.set_focus().ok();
        }
        Err(e) => eprintln!("hop: could not open Settings ({e})"),
    }
}

/// ⌘, in the palette.
#[tauri::command]
pub fn open_settings(app: AppHandle) {
    open(&app);
}

/// Esc in Settings.
#[tauri::command]
pub fn close_settings(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        w.close().ok();
    }
}

/// Whether the Settings window exists: the palette then hides without hiding the whole app, which would hide Settings.
pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(LABEL).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Defaults for a file hop must not treat as a first run (corrupt or unreadable): start at login is left alone.
    fn hands_off() -> Settings {
        Settings { login_applied: true, ..Settings::default() }
    }

    #[test]
    fn missing_file_is_defaults_and_a_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let (s, first) = load(&tmp.path().join("settings.json"));
        assert!(first);
        assert_eq!(s, Settings::default());
        assert_eq!((s.hotkey.as_str(), s.theme.as_str(), s.palette_style.as_str()), ("Alt+Space", "system", "compact"));
        assert!(s.daily_update_check && s.extra_folders.is_empty() && s.ignored_apps.is_empty() && !s.login_applied);
    }

    #[test]
    fn round_trips_and_writes_the_version() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("sub/settings.json");
        let s = Settings {
            hotkey: "Control+Alt+KeyK".into(),
            extra_folders: vec!["/Volumes/Apps".into()],
            ignored_apps: vec!["/Applications/Helper.app".into()],
            theme: "dark".into(),
            palette_style: "roomy".into(),
            daily_update_check: false,
            login_applied: true,
            ..Settings::default()
        };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), (s, false));
        assert!(std::fs::read_to_string(&p).unwrap().contains(r#""version": 1"#));
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    fn login_is_applied_until_a_file_says_it_was() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"paletteStyle":"roomy"}"#).unwrap(); // M2
        let (s, first) = load(&p);
        assert_eq!(s.palette_style, "roomy");
        assert!(first);
        // a dev build saved its settings (it may not turn login on): the installed app still has to
        std::fs::write(&p, r#"{"version":1,"theme":"dark"}"#).unwrap();
        assert!(load(&p).1);
        std::fs::write(&p, r#"{"version":1,"loginApplied":true}"#).unwrap();
        assert!(!load(&p).1);
    }

    #[test]
    fn unknown_keys_survive_a_save_and_bad_values_fall_back_one_key_at_a_time() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        let file = json!({"version": 2, "future": {"a": 1}, "theme": "sepia", "paletteStyle": 3, "hotkey": "Alt+Nope",
            "extraFolders": ["relative/path"], "ignoredApps": ["/Applications/X.app"], "dailyUpdateCheck": false, "loginApplied": true});
        std::fs::write(&p, file.to_string()).unwrap();
        let (s, first) = load(&p);
        assert!(!first);
        assert_eq!(s.other, serde_json::Map::from_iter([("future".to_string(), json!({"a": 1}))]));
        assert_eq!(s, Settings { ignored_apps: vec!["/Applications/X.app".into()], daily_update_check: false, other: s.other.clone(), ..hands_off() });
        let (s2, _) = merge(&s, &json!({"theme": "dark"}));
        save(&p, &s2).unwrap();
        let back: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!((&back["future"], &back["theme"]), (&json!({"a": 1}), &json!("dark")));
    }

    #[test]
    fn corrupt_file_is_kept_aside_and_reads_as_defaults_without_a_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        let latin1: &[u8] = b"{\"extraFolders\":[\"/Volumes/Caf\xe9\"]}";
        for bad in [b"{not json".as_slice(), b"[]", b"null", latin1] {
            std::fs::write(&p, bad).unwrap();
            assert_eq!(load(&p), (hands_off(), false), "{}", String::from_utf8_lossy(bad));
            assert_eq!(std::fs::read(p.with_extension("json.corrupt")).unwrap(), bad);
        }
    }

    #[test]
    fn unreadable_file_reads_as_defaults_without_a_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::create_dir(&p).unwrap(); // EISDIR: as unreadable as EACCES, and testable as any user
        assert_eq!(load(&p), (hands_off(), false));
        assert!(!p.with_extension("json.corrupt").exists());
    }

    #[test]
    fn merge_reports_the_keys_it_refused() {
        let cur = Settings::default();
        let (s, bad) = merge(&cur, &json!({"theme": "light", "hotkey": "Super+Shift+Space", "version": 7, "loginApplied": true, "junk": 1}));
        assert_eq!((s.theme.as_str(), s.hotkey.as_str(), s.version), ("light", "Super+Shift+Space", VERSION));
        assert!(bad.is_empty() && !s.login_applied && s.other.is_empty(), "the window cannot set loginApplied or add keys");
        let (s, bad) = merge(&cur, &json!({"hotkey": "Space+Alt", "paletteStyle": "huge", "ignoredApps": "x", "dailyUpdateCheck": false}));
        assert_eq!(s, Settings { daily_update_check: false, ..cur.clone() });
        assert_eq!(bad, ["hotkey", "ignoredApps", "paletteStyle"]);
        assert_eq!(merge(&cur, &json!({"hotkey": ""})).0.hotkey, "", "no hotkey is allowed: the tray is the way in");
    }

    #[test]
    fn extra_folders_refuse_the_whole_disk_and_home() {
        let home = std::env::var("HOME").unwrap();
        for f in ["/", "/Users", "/Users/", "/users", "/Users/..", "/Volumes", "/System/Volumes/Data", "/System/Volumes/Data/Users", &format!("/System/Volumes/Data{home}"), home.as_str()] {
            assert!(too_broad(f), "{f}");
            assert_eq!(merge(&Settings::default(), &json!({"extraFolders": [f]})).1, ["extraFolders"], "{f}");
        }
        for f in ["/Volumes/Apps", "/opt/apps", &format!("{home}/Apps")] {
            assert!(!too_broad(f), "{f}");
        }
    }

    #[test]
    fn settings_default_turns_commands_on() {
        assert!(Settings::default().system_commands);
        assert_eq!(serde_json::to_value(Settings::default()).unwrap()["systemCommands"], json!(true));
    }

    #[test]
    fn old_settings_file_keeps_commands_on() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        // a 0.2.0 file: every key it knew, none of the new ones
        std::fs::write(&p, r#"{"version":1,"hotkey":"Alt+Space","extraFolders":[],"ignoredApps":[],"theme":"dark","paletteStyle":"roomy","dailyUpdateCheck":false,"loginApplied":true}"#).unwrap();
        let (s, _) = load(&p);
        assert!(s.system_commands && s.other.is_empty());
        assert_eq!((s.theme.as_str(), s.daily_update_check), ("dark", false));
        // the switch is saved and read back; a wrong type falls back to on
        let (off, bad) = merge(&s, &json!({"systemCommands": false}));
        assert!(bad.is_empty() && !off.system_commands);
        save(&p, &off).unwrap();
        assert!(!load(&p).0.system_commands);
        std::fs::write(&p, r#"{"version":1,"systemCommands":"no","loginApplied":true}"#).unwrap();
        assert!(load(&p).0.system_commands);
    }

    #[test]
    fn only_a_missing_login_applied_is_a_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        for (text, first) in [(r#"{"version":1}"#, true), (r#"{"version":1,"loginApplied":false}"#, true), (r#"{"version":1,"loginApplied":1}"#, false), (r#"{"version":1,"loginApplied":true}"#, false)] {
            std::fs::write(&p, text).unwrap();
            assert_eq!(load(&p).1, first, "{text}");
        }
    }
}
