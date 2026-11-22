//! In-app updates (plan §7), ported from velo: a check 30 s after launch, then daily while Settings › Check every day
//! is on; a manual check from Settings and the menu-bar icon; install on request from the What's new window. Delivery
//! goes through tauri-plugin-updater, which verifies the package against the minisign public key in `tauri.conf.json`
//! (with `requireSignedVersion`). Nothing pops up by itself: a found update shows as the palette's footer pill (read at
//! the next show, `panel::look`) and as the menu-bar item's text.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Let startup indexing settle before touching GitHub.
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 3600);
/// How often the checker thread wakes to re-read the clock and the setting: turning the daily check back on takes
/// effect within this. One wake per ten minutes keeps idle CPU at zero.
const WAKE_POLL: Duration = Duration::from_secs(600);
const _: () = assert!(FIRST_CHECK_DELAY.as_secs() < CHECK_INTERVAL.as_secs());

/// Idle time between bytes before a check or download is abandoned. Without it a half-open socket leaves `busy` set
/// until hop restarts.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

pub const DEBUG_BUILD: &str = "Updates are off in development builds.";
pub const BUSY: &str = "An update check or install is already running.";
pub const NO_UPDATE: &str = "No update to install.";
pub const NEWER: &str = "hop found a newer version since this window opened.";

/// What the last check found. The plugin's `Update` handle is kept so "Install" needs no second round-trip to the feed.
#[derive(Default)]
pub struct UpdateState {
    pub available: Option<Update>,
    /// A check or an install is running; a second request is refused.
    pub busy: bool,
    /// The install as the What's new window shows it, and that window's channel (crate::whatsnew).
    pub phase: Phase,
    pub watcher: Option<Channel<Phase>>,
}

/// Where an install stands, for the What's new window. After `Failed` the update is kept: the window offers a retry.
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Phase {
    #[default]
    Idle,
    Downloading { downloaded: u64, total: Option<u64> },
    Installing,
    Failed { message: String },
}

/// Records `phase` and sends it to the What's new window, if one listens. Its channel reaches only that webview.
pub(crate) fn set_phase(st: &mut UpdateState, phase: Phase) {
    if let Some(ch) = &st.watcher
        && ch.send(phase.clone()).is_err()
    {
        st.watcher = None;
    }
    st.phase = phase;
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    /// The running version, as the updater compared it.
    pub current_version: String,
    /// The release notes from the feed (`latest.json` `notes`), empty if none. Untrusted text: the What's new window
    /// renders it without HTML.
    pub notes: String,
}

impl From<&Update> for UpdateInfo {
    fn from(u: &Update) -> Self {
        Self { version: u.version.clone(), current_version: u.current_version.clone(), notes: u.body.clone().unwrap_or_default() }
    }
}

/// Which `n`th of the download `done` is in (`n` = 100: percent), or 256 KiB steps when the size is unknown. Progress is
/// sent when the bucket changes, not on every chunk.
fn bucket(done: u64, total: Option<u64>, n: u64) -> u64 {
    match total {
        Some(t) if t > 0 => done.min(t) * n / t,
        _ => done >> 18,
    }
}

/// Time since the last check: by the wall clock (None when the clock was set back past it) and awake (monotonic, which
/// stops while the Mac sleeps).
#[derive(Clone, Copy)]
pub struct Since {
    pub wall: Option<Duration>,
    pub awake: Duration,
}

/// Whether the checker runs a check at this wake. `last`: None until the first check (the thread has already slept
/// `FIRST_CHECK_DELAY`). The wall clock makes a laptop that sleeps most of the day still check daily; the awake bound
/// ends the wait if the clock was set back.
pub fn due(enabled: bool, last: Option<Since>) -> bool {
    enabled && last.is_none_or(|s| s.wall.is_some_and(|w| w >= CHECK_INTERVAL) || s.awake >= CHECK_INTERVAL)
}

pub(crate) fn state(app: &AppHandle) -> MutexGuard<'_, UpdateState> {
    app.state::<Mutex<UpdateState>>().inner().lock().unwrap_or_else(|p| p.into_inner())
}

/// The version waiting to install, if any: the palette's pill and Settings show it.
pub fn pending(app: &AppHandle) -> Option<String> {
    app.try_state::<Mutex<UpdateState>>()?.lock().unwrap_or_else(|p| p.into_inner()).available.as_ref().map(|u| u.version.clone())
}

/// Claims the busy flag; `false` means another check or install is already running.
fn begin(st: &mut UpdateState) -> bool {
    !std::mem::replace(&mut st.busy, true)
}

pub fn setup(app: &AppHandle) {
    app.manage(Mutex::new(UpdateState::default()));
    spawn_checker(app.clone());
}

/// Release builds only, so `tauri dev` never hits the feed or replaces its own `target/debug` binary.
fn spawn_checker(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK_DELAY);
        let mut last: Option<(SystemTime, Instant)> = None;
        loop {
            let since = last.map(|(w, a)| Since { wall: w.elapsed().ok(), awake: a.elapsed() });
            if due(crate::settings::current(&app).daily_update_check, since) {
                // a background failure is never shown (offline, GitHub down, a manual check running): one log line,
                // and the next wake tries again instead of waiting a day
                match tauri::async_runtime::block_on(check(&app)) {
                    Ok(_) => last = Some((SystemTime::now(), Instant::now())),
                    Err(e) => eprintln!("hop: update check failed: {e}"),
                }
            }
            std::thread::sleep(WAKE_POLL);
        }
    });
}

/// Asks the feed for a newer version. `None`: up to date. A second check while one runs is refused with `BUSY`.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    if cfg!(debug_assertions) {
        return Err(DEBUG_BUILD.into());
    }
    if !begin(&mut state(app)) {
        return Err(BUSY.into());
    }
    // the lock is released here; the network call runs without it
    let result = async { app.updater_builder().configure_client(|c| c.read_timeout(READ_TIMEOUT)).build()?.check().await }.await;
    let mut st = state(app);
    st.busy = false;
    let update = result.map_err(|e| e.to_string())?;
    let info = update.as_ref().map(UpdateInfo::from);
    if info.is_some() {
        set_phase(&mut st, Phase::Idle); // a fresh update: an earlier failure is over
    }
    st.available = update;
    drop(st);
    refresh_tray(app);
    Ok(info)
}

/// Downloads and installs the stored update, then relaunches. `expected`: the version the user saw; a newer one found
/// since is refused, so it is never installed unseen.
pub async fn install(app: &AppHandle, expected: &str) -> Result<(), String> {
    let update = {
        let mut st = state(app);
        let update = st.available.clone().ok_or(NO_UPDATE)?;
        if update.version != expected {
            return Err(NEWER.into());
        }
        if !begin(&mut st) {
            return Err(BUSY.into());
        }
        set_phase(&mut st, Phase::Downloading { downloaded: 0, total: None });
        update
    };
    let dir = app.path().app_data_dir().ok();
    let (mut done, mut shown) = (0u64, None);
    let on_chunk = |len: usize, total: Option<u64>| {
        done += len as u64;
        let b = bucket(done, total, 100);
        if shown != Some(b) {
            shown = Some(b);
            set_phase(&mut state(app), Phase::Downloading { downloaded: done, total });
        }
    };
    // the plugin verifies the signature after this, then swaps the bundle. The mark makes the relaunch wait for this
    // process's instance lock; written after the download, so its ten minutes do not run out on a slow link
    let on_finish = || {
        set_phase(&mut state(app), Phase::Installing);
        if let Some(d) = &dir {
            crate::login::mark_restart(d);
        }
    };
    let result = update.download_and_install(on_chunk, on_finish).await;
    let mut st = state(app);
    st.busy = false;
    if let Err(e) = result {
        if let Some(d) = &dir {
            crate::login::clear_restart_mark(d);
        }
        eprintln!("hop: update install failed: {e}");
        set_phase(&mut st, Phase::Failed { message: format!("The update failed: {e}") });
        return Err(e.to_string());
    }
    drop(st);
    // never restart under the user's typing (the What's new window may have been closed during the download)
    // shortcut: polls on a runtime worker thread; an event from the palette's hide would avoid it if that ever matters
    while crate::panel::is_shown(app) {
        std::thread::sleep(Duration::from_millis(250));
    }
    app.restart();
}

/// The menu-bar item: "Check for Updates…", or "Update to hop X…" once one is found.
pub struct TrayItem(pub tauri::menu::MenuItem<tauri::Wry>);

pub fn tray_text(pending: Option<&str>) -> String {
    pending.map_or_else(|| "Check for Updates…".into(), |v| format!("Update to hop {v}…"))
}

fn refresh_tray(app: &AppHandle) {
    if let Some(item) = app.try_state::<TrayItem>() {
        item.0.set_text(tray_text(pending(app).as_deref())).ok();
    }
}

/// The menu-bar item. With an update found: the What's new window. Otherwise a check whose answer the user asked for:
/// What's new when one is found, else a message (up to date, busy or the error).
pub fn tray_clicked(app: &AppHandle) {
    if pending(app).is_some() {
        crate::whatsnew::open(app);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
        let say = |kind, text: String| app.dialog().message(text).title("hop").kind(kind).show(|_| {});
        match check(&app).await {
            Ok(Some(_)) => {
                let a = app.clone();
                app.run_on_main_thread(move || crate::whatsnew::open(&a)).ok();
            }
            Ok(None) => say(MessageDialogKind::Info, format!("hop {} is up to date.", app.package_info().version)),
            Err(e) => say(MessageDialogKind::Warning, format!("Could not check for updates: {e}")),
        }
    });
}

/// Settings › Check now. The answer shows inline in Settings.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    check(&app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_whatsnew_window_knows_the_stale_install_errors() {
        let api = include_str!("../../src/whatsnew/api.ts");
        assert!(api.contains(&format!("\"{NO_UPDATE}\"")) && api.contains(&format!("\"{NEWER}\"")));
    }

    #[test]
    fn checks_first_then_daily_and_only_while_on() {
        let h = |n: u64| Duration::from_secs(n * 3600);
        assert!(due(true, None), "the first check, after the startup delay");
        assert!(!due(false, None), "the daily check is off");
        let since = |wall: Option<Duration>, awake| Some(Since { wall, awake });
        assert!(!due(true, since(Some(h(23)), h(23))));
        assert!(due(true, since(Some(h(24)), h(2))), "a laptop asleep most of the day still checks daily");
        assert!(due(true, since(None, h(24))), "a clock set back: the awake time ends the wait");
        assert!(!due(true, since(None, h(1))));
        assert!(!due(false, since(Some(h(48)), h(48))), "turned off: no check however long");
    }

    #[test]
    fn a_second_check_or_install_is_refused_while_one_runs() {
        let mut st = UpdateState::default();
        assert!(begin(&mut st));
        assert!(!begin(&mut st));
        st.busy = false;
        assert!(begin(&mut st));
    }

    /// The shape src/whatsnew/api.ts reads.
    #[test]
    fn phase_wire_format() {
        let j = |p: Phase| serde_json::to_value(p).unwrap();
        assert_eq!(j(Phase::Idle), serde_json::json!({ "kind": "idle" }));
        assert_eq!(j(Phase::Downloading { downloaded: 5, total: None }), serde_json::json!({ "kind": "downloading", "downloaded": 5, "total": null }));
        assert_eq!(j(Phase::Installing), serde_json::json!({ "kind": "installing" }));
        assert_eq!(j(Phase::Failed { message: "x".into() }), serde_json::json!({ "kind": "failed", "message": "x" }));
    }

    #[test]
    fn progress_buckets() {
        assert_eq!(bucket(0, Some(1000), 100), 0);
        assert_eq!(bucket(499, Some(1000), 4), 1);
        assert_eq!(bucket(1000, Some(1000), 4), 4);
        assert_eq!(bucket(2000, Some(1000), 100), 100); // a server that lied about the size
        assert_eq!(bucket(300 * 1024, None, 100), 1); // unknown size: 256 KiB steps
        assert_eq!(bucket(10, Some(0), 100), 0);
    }

    #[test]
    fn tray_item_names_the_pending_version() {
        assert_eq!(tray_text(None), "Check for Updates…");
        assert_eq!(tray_text(Some("0.2.0")), "Update to hop 0.2.0…");
    }

    /// The feed and key the installed app trusts: a release must not ship with a local test feed or plain http.
    #[test]
    fn updater_config_is_the_signed_github_feed() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let u = &conf["plugins"]["updater"];
        assert_eq!(u["endpoints"], serde_json::json!(["https://github.com/cef62/hop/releases/latest/download/latest.json"]));
        assert_eq!(u["requireSignedVersion"], true);
        assert!(u["pubkey"].as_str().is_some_and(|k| !k.is_empty()));
        assert!(u.get("dangerousInsecureTransportProtocol").is_none());
    }
}
