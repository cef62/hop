//! The index of what the palette finds: apps, found by walking the roots and named as the Finder does, plus System
//! Settings sections (`panes`) and system commands (`commands`). Kept in memory and rescanned when a root changes. The frontend gets the list once
//! (`list_apps`) and again on every `apps-changed` event.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    App,
    Pane,
    Command,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub kind: Kind,
    /// What `run` and frecency key on: an app's path (so `history.json` from before items carries over), `pane:<bundle
    /// id>` for a section, `cmd:<name>` for a command.
    pub id: String,
    /// What the Finder shows (localized), without `.app`.
    pub name: String,
    /// The file name without `.app`, when it differs from `name`: an English name on an Italian system.
    pub alt_names: Vec<String>,
    /// The bundle whose icon the row shows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The prompt a second Enter must confirm.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
}

/// The index and its generation. The generation lets the frontend drop a `list_apps` reply that arrives after a newer
/// `apps-changed` event.
#[derive(Serialize, Clone, Default)]
pub struct Index {
    pub generation: u64,
    pub items: Vec<Item>,
}

/// Levels below a root that can hold an app: `/Applications/X.app` and `/Applications/Vendor/X.app`.
const DEPTH: usize = 2;

/// The built-in roots, then the extra folders from Settings.
pub fn roots(extra: &[String]) -> Vec<PathBuf> {
    let mut r: Vec<PathBuf> = ["/Applications", "/System/Applications", "/System/Applications/Utilities", "/Applications/Utilities"]
        .into_iter()
        .map(PathBuf::from)
        .collect();
    if let Some(home) = std::env::var_os("HOME") {
        r.push(Path::new(&home).join("Applications"));
    }
    r.extend(extra.iter().map(PathBuf::from));
    r
}

/// `apps` without the ones on the ignore list, matched by the path as indexed or as resolved (the picker can return
/// either side of a symlink).
pub fn without_ignored(apps: Vec<Item>, ignored: &[String]) -> Vec<Item> {
    if ignored.is_empty() {
        return apps;
    }
    let real = |p: &str| std::fs::canonicalize(p).ok().map(|c| c.to_string_lossy().into_owned());
    let ignored: HashSet<String> = ignored.iter().flat_map(|p| [Some(p.clone()), real(p)]).flatten().collect();
    apps.into_iter().filter(|a| !ignored.contains(&a.id) && !real(&a.id).is_some_and(|r| ignored.contains(&r))).collect()
}

fn is_app(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

/// Every `.app` under `roots`, at most `DEPTH` levels down, never inside an `.app`. Symlinks are followed (an app or a
/// vendor folder can be a link); a bundle reached twice (link and target, or nested roots) is kept once, under the
/// first path found. Unreadable or missing roots are skipped.
pub fn scan(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        walk(root, DEPTH, &mut seen, &mut out);
    }
    out
}

fn walk(dir: &Path, depth: usize, seen: &mut HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort(); // stable order across scans, so the first-found path of a duplicate does not flip
    for p in entries {
        if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        // follows symlinks; a dangling link has no metadata and is skipped
        if !std::fs::metadata(&p).is_ok_and(|m| m.is_dir()) {
            continue;
        }
        if is_app(&p) {
            if seen.insert(std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone())) {
                out.push(p);
            }
        } else if depth > 1 {
            walk(&p, depth - 1, seen, out);
        }
    }
}

/// `display` is the Finder's name for the bundle (`NSFileManager displayNameAtPath:`), which may or may not end in
/// `.app`; the file name becomes an alternative search key when it differs.
pub fn app_entry(path: &Path, display: &str) -> Item {
    let strip = |s: &str| s.strip_suffix(".app").unwrap_or(s).to_string();
    let name = strip(display);
    let file = path.file_name().map(|n| strip(&n.to_string_lossy())).unwrap_or_default();
    let name = if name.is_empty() { file.clone() } else { name };
    let alt_names = if file != name && !file.is_empty() { vec![file] } else { vec![] };
    let path = path.to_string_lossy().into_owned();
    Item { kind: Kind::App, id: path.clone(), name, alt_names, path: Some(path), confirm: None }
}

#[cfg(target_os = "macos")]
pub fn display_name(path: &Path) -> String {
    use objc2_foundation::{NSFileManager, NSString};
    NSFileManager::defaultManager().displayNameAtPath(&NSString::from_str(&path.to_string_lossy())).to_string()
}

#[cfg(not(target_os = "macos"))]
pub fn display_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn index(roots: &[PathBuf]) -> Vec<Item> {
    scan(roots).iter().map(|p| app_entry(p, &display_name(p))).collect()
}

/// A change at `path` can add, remove or rename an app only within `DEPTH` levels of a root: changes inside a bundle
/// (an app updating itself writes thousands of files) are ignored. `roots` are canonical, as FSEvents reports paths.
fn relevant(path: &Path, roots: &[PathBuf]) -> bool {
    let in_bundle = |rest: &Path| rest.parent().is_some_and(|p| p.components().any(|c| is_app(Path::new(c.as_os_str()))));
    roots.iter().any(|r| path.strip_prefix(r).is_ok_and(|rest| rest.components().count() <= DEPTH && !in_bundle(rest)))
}

/// The index: the apps not ignored, the sections, then the system commands when `commands` is on. The ignore list is
/// for apps: ignoring System Settings leaves its sections in (their rows just lose the icon).
fn assemble(apps: Vec<Item>, ignored: &[String], panes: Vec<Item>, commands: bool) -> Vec<Item> {
    let mut items = without_ignored(apps, ignored);
    items.extend(panes);
    if commands {
        items.extend(crate::commands::items());
    }
    items
}

/// One scan at a time: two quick Settings changes must not publish their scans out of order.
static SCANNING: Mutex<()> = Mutex::new(());

/// Rescans with the current Settings and publishes the result: the state for `list_apps`, the event for a running
/// frontend.
fn refresh(app: &AppHandle) {
    let _one = SCANNING.lock().unwrap();
    let s = crate::settings::current(app);
    let items = assemble(index(&roots(&s.extra_folders)), &s.ignored_apps, crate::panes::index(), s.system_commands);
    let snapshot = {
        let state = app.state::<Mutex<Index>>();
        let mut idx = state.lock().unwrap();
        *idx = Index { generation: idx.generation + 1, items };
        idx.clone()
    };
    app.emit_to("main", "apps-changed", snapshot).ok();
}

/// How long the watcher waits for a burst of events to go quiet before it rescans, and the longest it waits.
const QUIET: Duration = Duration::from_millis(500);
const MAX_WAIT: Duration = Duration::from_secs(5);

/// Watches each existing root and rescans after a burst of relevant events. A plain notify watcher and a blocking
/// channel, not notify-debouncer-full: its file-id cache walks every file under the roots (hundreds of MB for
/// /Applications) and its tick thread wakes up even when nothing changes. Lost events (`need_rescan`, watcher errors)
/// rescan too.
// shortcut: a root that does not exist at startup (no ~/Applications) is not watched until hop restarts
fn watch(roots: &[PathBuf], on_change: impl Fn() + Send + 'static) -> notify::Result<RecommendedWatcher> {
    let roots: Vec<PathBuf> = roots.iter().filter_map(|r| std::fs::canonicalize(r).ok()).collect();
    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;
    for r in &roots {
        // FSEvents watches a whole tree in one stream; depth is filtered in `relevant`
        if let Err(e) = watcher.watch(r, RecursiveMode::Recursive) {
            eprintln!("hop: cannot watch {} ({e})", r.display());
        }
    }
    let changed = move |ev: notify::Result<notify::Event>| ev.map_or(true, |e| e.need_rescan() || e.paths.iter().any(|p| relevant(p, &roots)));
    std::thread::spawn(move || {
        // blocks while nothing happens: no idle wakeups. Ends when the watcher (kept in app state) is dropped
        while let Ok(ev) = rx.recv() {
            let (start, mut dirty) = (Instant::now(), changed(ev));
            while start.elapsed() < MAX_WAIT {
                match rx.recv_timeout(QUIET) {
                    Ok(ev) => dirty |= changed(ev),
                    Err(_) => break,
                }
            }
            if dirty {
                on_change();
            }
        }
    });
    Ok(watcher)
}

/// The live watcher. Replacing it drops the old one, which ends its thread.
type Watching = Mutex<Option<RecommendedWatcher>>;

fn rewatch(app: &AppHandle) {
    let refresher = app.clone();
    let w = match watch(&roots(&crate::settings::current(app).extra_folders), move || refresh(&refresher)) {
        Ok(w) => Some(w),
        Err(e) => {
            eprintln!("hop: app watcher failed ({e}); new apps appear after a restart");
            None
        }
    };
    *app.state::<Watching>().lock().unwrap() = w;
}

/// Starts the first scan and the watcher off the main thread, so neither delays the hotkey or the first show.
pub fn setup(app: &AppHandle) {
    app.manage(Mutex::new(Index::default()));
    app.manage(Watching::default());
    let app = app.clone();
    std::thread::spawn(move || {
        let t = std::time::Instant::now();
        refresh(&app);
        eprintln!("hop: indexed {} items in {} ms", app.state::<Mutex<Index>>().lock().unwrap().items.len(), t.elapsed().as_millis());
        rewatch(&app);
    });
}

/// After Settings changed the extra folders (`folders`, which also re-targets the watcher), the ignore list or the
/// "System commands" switch.
pub fn reload(app: &AppHandle, folders: bool) {
    let app = app.clone();
    std::thread::spawn(move || {
        refresh(&app);
        if folders {
            rewatch(&app);
        }
    });
}

/// Whether `path` is an app in the index: the icon scheme takes paths from the webview and serves only these.
pub fn known_app(app: &AppHandle, path: &str) -> bool {
    has_app(&app.state::<Mutex<Index>>().lock().unwrap().items, path)
}

/// Apps only: a section carries the System Settings path for its icon, which must not make an ignored app's icon
/// servable.
fn has_app(items: &[Item], path: &str) -> bool {
    items.iter().any(|i| i.kind == Kind::App && i.path.as_deref() == Some(path))
}

/// The index entry for `id`: `run` acts only on these, since the id comes from the webview.
pub fn item(app: &AppHandle, id: &str) -> Option<Item> {
    find(&app.state::<Mutex<Index>>().lock().unwrap().items, id).cloned()
}

fn find<'a>(items: &'a [Item], id: &str) -> Option<&'a Item> {
    items.iter().find(|i| i.id == id)
}

#[tauri::command]
pub fn list_apps(app: AppHandle) -> Index {
    app.state::<Mutex<Index>>().lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn mkapp(p: &Path) {
        fs::create_dir_all(p.join("Contents/MacOS")).unwrap();
    }

    fn names(found: &[PathBuf], base: &Path) -> Vec<String> {
        found.iter().map(|p| p.strip_prefix(base).unwrap().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn walks_two_levels_never_into_bundles_follows_links_and_dedupes() {
        let tmp = tempfile::tempdir().unwrap();
        let (root, other) = (tmp.path().join("Applications"), tmp.path().join("elsewhere"));
        mkapp(&root.join("Safari.app"));
        mkapp(&root.join("Adobe Photoshop/Photoshop.app")); // vendor folder
        mkapp(&root.join("Adobe Photoshop/Helpers/Deep.app")); // three levels: out of reach
        mkapp(&root.join("Xcode.app/Contents/Applications/Simulator.app")); // app inside an app: not descended
        mkapp(&other.join("Linked.app"));
        std::os::unix::fs::symlink(other.join("Linked.app"), root.join("Linked.app")).unwrap();
        std::os::unix::fs::symlink(root.join("Safari.app"), root.join("Web.app")).unwrap(); // same bundle twice
        std::os::unix::fs::symlink(tmp.path().join("gone.app"), root.join("Dangling.app")).unwrap();
        fs::create_dir_all(root.join("Notes.txt.d")).unwrap(); // a plain folder with no app
        fs::write(root.join("README.app"), "a file named like an app").unwrap();
        mkapp(&root.join(".Hidden.app"));
        mkapp(&root.join("Utilities/Terminal.app"));

        let found = scan(&[root.clone(), root.join("Utilities"), tmp.path().join("missing")]);
        assert_eq!(
            names(&found, &root),
            ["Adobe Photoshop/Photoshop.app", "Linked.app", "Safari.app", "Utilities/Terminal.app", "Xcode.app"],
            "{found:?}"
        );
    }

    #[test]
    fn localized_display_name_is_the_name_and_file_name_the_alt_name() {
        let p = Path::new("/System/Applications/System Settings.app");
        assert_eq!(app_entry(p, "Impostazioni di Sistema.app"), Item {
            kind: Kind::App,
            id: p.to_string_lossy().into(),
            name: "Impostazioni di Sistema".into(),
            alt_names: vec!["System Settings".into()],
            path: Some(p.to_string_lossy().into()),
            confirm: None,
        });
        // the Finder hides the extension: same name, no alt
        assert_eq!(app_entry(p, "System Settings").alt_names, Vec::<String>::new());
        // no display name: the file name stands in
        assert_eq!(app_entry(p, "").name, "System Settings");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn display_name_reads_the_bundle_localization() {
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("Plain.app");
        mkapp(&app);
        assert_eq!(app_entry(&app, &display_name(&app)).name, "Plain");

        // a bundle that declares a localized name for the system language (English on the owner's Mac)
        let loc = tmp.path().join("Calc.app");
        fs::create_dir_all(loc.join("Contents/Resources/en.lproj")).unwrap();
        fs::create_dir_all(loc.join("Contents/Resources/Base.lproj")).unwrap();
        fs::write(
            loc.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>dev.hop.test.calc</string><key>CFBundleName</key><string>Calc</string>
<key>CFBundlePackageType</key><string>APPL</string><key>LSHasLocalizedDisplayName</key><true/>
</dict></plist>"#,
        )
        .unwrap();
        for l in ["en", "Base"] {
            fs::write(loc.join(format!("Contents/Resources/{l}.lproj/InfoPlist.strings")), "CFBundleDisplayName = \"Calcolatrice\";\n").unwrap();
        }
        let e = app_entry(&loc, &display_name(&loc));
        assert_eq!((e.name.as_str(), e.alt_names.as_slice()), ("Calcolatrice", ["Calc".to_string()].as_slice()));
    }

    #[test]
    fn watcher_reports_a_new_app_once_per_burst_and_ignores_changes_inside_bundles() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        mkapp(&root.join("Old.app"));
        let (tx, rx) = std::sync::mpsc::channel();
        let _w = watch(std::slice::from_ref(&root), move || tx.send(()).unwrap()).unwrap();
        // FSEvents can still deliver the setup above (it batches by latency): let that burst pass
        rx.recv_timeout(Duration::from_millis(1500)).ok();
        for i in 0..200 {
            fs::write(root.join(format!("Old.app/Contents/MacOS/f{i}")), "x").unwrap(); // an app updating itself
        }
        assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err(), "changes inside a bundle rescanned");
        for i in 0..50 {
            mkapp(&root.join(format!("Vendor/New{i}.app"))); // an installer dropping many apps at once
        }
        rx.recv_timeout(Duration::from_secs(5)).expect("no rescan after new apps");
        assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err(), "one burst rescanned more than once");
    }

    #[test]
    fn ignored_apps_leave_the_index_by_either_path() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real/Tool.app");
        mkapp(&real);
        std::os::unix::fs::symlink(&real, tmp.path().join("Tool.app")).unwrap();
        let entry = |p: &Path| app_entry(p, "");
        let apps = vec![entry(&tmp.path().join("Tool.app")), entry(Path::new("/Applications/Safari.app")), entry(Path::new("/Applications/Mail.app"))];
        let paths = |v: Vec<Item>| v.into_iter().map(|a| a.name).collect::<Vec<_>>();
        assert_eq!(paths(without_ignored(apps.clone(), &[])), ["Tool", "Safari", "Mail"]);
        assert_eq!(paths(without_ignored(apps.clone(), &["/Applications/Mail.app".into()])), ["Tool", "Safari"]);
        // ignored through the link's target, indexed through the link
        assert_eq!(paths(without_ignored(apps, &[real.to_string_lossy().into()])), ["Safari", "Mail"]);
    }

    #[test]
    fn app_items_use_the_path_as_id() {
        let p = Path::new("/Applications/X.app");
        let e = app_entry(p, "X");
        assert_eq!((e.kind, e.id.as_str(), e.path.as_deref()), (Kind::App, "/Applications/X.app", Some("/Applications/X.app")));
    }

    #[test]
    fn known_app_ignores_other_kinds() {
        let settings = "/System/Applications/System Settings.app";
        let pane = Item { kind: Kind::Pane, id: "pane:a".into(), name: "A".into(), alt_names: vec![], path: Some(settings.into()), confirm: None };
        assert!(!has_app(std::slice::from_ref(&pane), settings), "a section's icon path is not an app");
        assert!(has_app(&[pane, app_entry(Path::new(settings), "")], settings));
    }

    #[test]
    fn ignoring_system_settings_keeps_its_sections_runnable() {
        let settings = "/System/Applications/System Settings.app";
        let pane = Item { kind: Kind::Pane, id: "pane:a".into(), name: "A".into(), alt_names: vec![], path: Some(settings.into()), confirm: None };
        let apps = vec![app_entry(Path::new(settings), ""), app_entry(Path::new("/Applications/Mail.app"), "")];
        let items = assemble(apps, &[settings.into()], vec![pane.clone()], false);
        assert_eq!(find(&items, "pane:a"), Some(&pane));
        assert_eq!(find(&items, settings), None);
        assert!(!has_app(&items, settings), "the ignored app's icon is served");
        assert!(find(&items, "/Applications/Mail.app").is_some());
        assert!(find(&items, "pane:b").is_none() && find(&items, "").is_none() && find(&items, "pane:").is_none());
    }

    #[test]
    fn index_without_commands_when_off() {
        let apps = vec![app_entry(Path::new("/Applications/Mail.app"), "")];
        let on = assemble(apps.clone(), &[], vec![], true);
        assert_eq!(on.iter().filter(|i| i.kind == Kind::Command).count(), 6);
        assert!(find(&on, "cmd:restart").is_some() && find(&on, "/Applications/Mail.app").is_some());
        let off = assemble(apps, &[], vec![], false);
        assert!(off.iter().all(|i| i.kind != Kind::Command), "{off:?}");
        // `run` looks ids up here: a command id is refused while the switch is off
        assert!(find(&off, "cmd:restart").is_none() && find(&off, "cmd:lock").is_none());
        assert!(find(&off, "/Applications/Mail.app").is_some());
    }

    #[test]
    fn extra_folders_join_the_roots() {
        let r = roots(&["/Volumes/Apps".into()]);
        assert_eq!(r[0], Path::new("/Applications"));
        assert_eq!(r.last().unwrap(), Path::new("/Volumes/Apps"));
    }

    #[test]
    fn only_changes_near_a_root_count() {
        let roots = [PathBuf::from("/Applications")];
        assert!(relevant(Path::new("/Applications/New.app"), &roots));
        assert!(relevant(Path::new("/Applications/Vendor/New.app"), &roots));
        assert!(relevant(Path::new("/Applications"), &roots));
        assert!(!relevant(Path::new("/Applications/Chrome.app/Contents/Frameworks/x"), &roots));
        assert!(!relevant(Path::new("/Applications/Chrome.app/Contents"), &roots));
        assert!(!relevant(Path::new("/Users/me/Applications/X.app"), &roots));
    }
}
