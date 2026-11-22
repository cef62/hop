//! Settings (plan §5) in `settings.json`, app config dir. Versioned; a missing file, unknown keys and keys with a wrong
//! type or value read as defaults, one key at a time; a file that is not a JSON object is kept as `settings.json.corrupt`
//! and hop starts from defaults. Written atomically. Start at login is not stored here: the LaunchAgent is the truth
//! (`login.rs`), as in velo.

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
    /// stored only until M5's updater reads it
    pub daily_update_check: bool,
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
            && abs(&self.ignored_apps)
    }
}

/// `patch`'s keys over `cur`, each taken only when it has the right type and a valid value; returns the keys refused
/// (unknown keys and `version` are skipped, not refused).
pub fn merge(cur: &Settings, patch: &Value) -> (Settings, Vec<String>) {
    let mut out = cur.clone();
    let mut bad = vec![];
    for (k, v) in patch.as_object().into_iter().flatten() {
        let mut json = serde_json::to_value(&out).expect("settings serialize");
        if k == "version" || json.get(k).is_none() {
            continue;
        }
        json[k] = v.clone();
        match serde_json::from_value::<Settings>(json) {
            Ok(s) if s.valid() => out = s,
            _ => bad.push(k.clone()),
        }
    }
    out.version = VERSION;
    (out, bad)
}

/// The settings, and whether start at login still has to be applied: no file, or one from before M3 (no `version`).
pub fn load(path: &Path) -> (Settings, bool) {
    let Ok(text) = std::fs::read_to_string(path) else { return (Settings::default(), true) };
    match serde_json::from_str::<Value>(&text) {
        Ok(v @ Value::Object(_)) => {
            let (s, bad) = merge(&Settings::default(), &v);
            if !bad.is_empty() {
                eprintln!("hop: {} has invalid {}; using the defaults for those", path.display(), bad.join(", "));
            }
            (s, v.get("version").is_none())
        }
        _ => {
            // kept for the owner to look at; the next save writes a fresh file
            let aside = path.with_extension("json.corrupt");
            match std::fs::copy(path, &aside) {
                Ok(_) => eprintln!("hop: {} is not valid; kept it as {} and started from defaults", path.display(), aside.display()),
                Err(e) => eprintln!("hop: {} is not valid and could not be kept aside ({e}); started from defaults", path.display()),
            }
            (Settings::default(), false)
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

/// Writes the current settings (after start at login was applied on a first run).
pub fn persist(app: &AppHandle) {
    if let Some(f) = file(app)
        && let Err(e) = save(&f, &current(app))
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
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> View {
    View {
        settings: current(&app),
        hotkey_error: crate::hotkey::error(),
        login: crate::login::enabled(&app),
        login_refusal: crate::login::refusal(),
        version: app.package_info().version.to_string(),
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
        return Err(format!("Not a valid value for {}", bad.join(", ")));
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
    if new.extra_folders != old.extra_folders || new.ignored_apps != old.ignored_apps {
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
    crate::panel::hide(app);
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
        .inner_size(520.0, 640.0)
        .min_inner_size(420.0, 420.0)
        .center()
        .build();
    match built {
        Ok(w) => {
            w.set_focus().ok();
        }
        Err(e) => eprintln!("hop: could not open Settings ({e})"),
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

    #[test]
    fn missing_file_is_defaults_and_a_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let (s, first) = load(&tmp.path().join("settings.json"));
        assert!(first);
        assert_eq!(s, Settings::default());
        assert_eq!((s.hotkey.as_str(), s.theme.as_str(), s.palette_style.as_str()), ("Alt+Space", "system", "compact"));
        assert!(s.daily_update_check && s.extra_folders.is_empty() && s.ignored_apps.is_empty());
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
            ..Settings::default()
        };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), (s, false));
        assert!(std::fs::read_to_string(&p).unwrap().contains(r#""version": 1"#));
        assert!(!p.with_extension("json.tmp").exists());
    }

    #[test]
    fn m2_file_keeps_its_palette_style_and_counts_as_a_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"paletteStyle":"roomy"}"#).unwrap();
        let (s, first) = load(&p);
        assert_eq!(s.palette_style, "roomy");
        assert!(first, "no version: start at login has never been applied");
    }

    #[test]
    fn unknown_keys_and_bad_values_fall_back_one_key_at_a_time() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        let file = json!({"version": 1, "future": true, "theme": "sepia", "paletteStyle": 3, "hotkey": "Alt+Nope",
            "extraFolders": ["relative/path"], "ignoredApps": ["/Applications/X.app"], "dailyUpdateCheck": false});
        std::fs::write(&p, file.to_string()).unwrap();
        let (s, first) = load(&p);
        assert!(!first);
        assert_eq!(s, Settings { ignored_apps: vec!["/Applications/X.app".into()], daily_update_check: false, ..Settings::default() });
    }

    #[test]
    fn corrupt_file_is_kept_aside_and_reads_as_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        for bad in ["{not json", "[]", "null"] {
            std::fs::write(&p, bad).unwrap();
            assert_eq!(load(&p), (Settings::default(), false), "{bad}");
            assert_eq!(std::fs::read_to_string(p.with_extension("json.corrupt")).unwrap(), bad);
        }
    }

    #[test]
    fn merge_reports_the_keys_it_refused() {
        let cur = Settings::default();
        let (s, bad) = merge(&cur, &json!({"theme": "light", "hotkey": "Super+Shift+Space", "version": 7}));
        assert_eq!((s.theme.as_str(), s.hotkey.as_str(), s.version), ("light", "Super+Shift+Space", VERSION));
        assert!(bad.is_empty());
        let (s, bad) = merge(&cur, &json!({"hotkey": "Space+Alt", "paletteStyle": "huge", "ignoredApps": "x", "dailyUpdateCheck": false}));
        assert_eq!(s, Settings { daily_update_check: false, ..cur.clone() });
        assert_eq!(bad, ["hotkey", "ignoredApps", "paletteStyle"]);
        assert_eq!(merge(&cur, &json!({"hotkey": ""})).0.hotkey, "", "no hotkey is allowed: the tray is the way in");
    }
}
