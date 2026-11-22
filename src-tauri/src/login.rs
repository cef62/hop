//! Start at login (a LaunchAgent through tauri-plugin-autostart, launched with `--hidden`) and the single-instance lock,
//! ported from velo. The LaunchAgent is the truth for "start at login": hop keeps no copy in its settings.

use std::path::Path;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// What only the login item passes. hop always starts hidden, so nothing reads it yet; it marks the item's launches.
pub const HIDDEN_ARG: &str = "--hidden";
/// The LaunchAgent's label and file name: the bundle identifier, so another tool called "hop" never shares it. A test
/// pins it to `tauri.conf.json`.
pub const AUTOSTART_NAME: &str = "dev.hop.app";

/// Takes the exclusive lock on `dir/instance.lock` for the rest of the process (the OS drops it at exit or crash, so no
/// stale lock is left). Retried for `wait`: the relaunch after an update can start before the old process has ended.
/// False only when another process holds it; a file system that cannot lock lets hop run.
///
/// Why not the single-instance plugin: its socket in shared /tmp could be squatted to block launch. A Finder, Dock or
/// Spotlight launch already reaches the running hop as `RunEvent::Reopen` (LaunchServices keeps one instance), but the
/// login item execs the binary outside LaunchServices: this lock makes that copy exit.
pub fn hold_instance_lock(dir: &Path, wait: Duration) -> bool {
    std::fs::create_dir_all(dir).ok();
    let Ok(f) = std::fs::File::options().create(true).truncate(false).write(true).open(dir.join("instance.lock")) else { return true };
    let until = std::time::Instant::now() + wait;
    loop {
        match f.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < until => std::thread::sleep(Duration::from_millis(100)),
            Err(std::fs::TryLockError::WouldBlock) => return false,
            Err(std::fs::TryLockError::Error(e)) => {
                eprintln!("hop: instance lock not taken ({e})");
                break;
            }
        }
    }
    std::mem::forget(f); // held until the process ends
    true
}

/// How long a start waits for another hop's instance lock: only the relaunch after an update (a fresh restart mark),
/// whose old process is still exiting. Any other start that loses exits at once.
pub fn lock_wait(fresh_mark: bool) -> Duration {
    if fresh_mark { Duration::from_secs(5) } else { Duration::ZERO }
}

const RESTART_MARK: &str = "restart-update";
const RESTART_MARK_TTL: Duration = Duration::from_secs(600);

/// Written when an update is downloaded and about to install; the relaunch reads it (`lock_wait`) and consumes it.
pub fn mark_restart(dir: &Path) {
    std::fs::write(dir.join(RESTART_MARK), b"").ok();
}

/// A failed install: the next start is not a relaunch.
pub fn clear_restart_mark(dir: &Path) {
    std::fs::remove_file(dir.join(RESTART_MARK)).ok();
}

/// Whether a mark younger than ten minutes exists; reads only.
pub fn restart_mark_fresh(dir: &Path) -> bool {
    std::fs::metadata(dir.join(RESTART_MARK)).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < RESTART_MARK_TTL)
}

/// True once for a mark younger than ten minutes; an older one (an install that never relaunched) is only deleted, so a
/// later login start never waits for the lock. hop starts hidden either way: the relaunch never shows a window.
pub fn take_restart_mark(dir: &Path) -> bool {
    let fresh = restart_mark_fresh(dir);
    clear_restart_mark(dir);
    fresh
}

/// Why a login item must not point at this executable, if so. The crate writes the path unquoted and unescaped: an App
/// Translocation path (a quarantined app run from where it was downloaded) is gone after a reboot, and `& < >` make the
/// plist invalid. A development build, or a release build run from cargo's `target` folder, is not the installed app
/// that shares the item's name.
pub fn login_item_refusal(exe: &str, debug_build: bool) -> Option<&'static str> {
    if debug_build {
        Some("Start at login is not available in a development build.")
    } else if exe.find("/target/").is_some_and(|i| ["/release/", "/debug/"].iter().any(|d| exe[i..].contains(d))) {
        Some("This hop runs from a build folder. Start at login is only for a hop installed in Applications.")
    } else if exe.contains("/AppTranslocation/") {
        Some("macOS is running hop from a temporary location. Move hop to Applications, open it from there, then turn Start at login on.")
    } else if exe.contains(['&', '<', '>']) {
        Some("hop's folder name contains & < or >, which a login item cannot hold. Move hop to Applications, then turn Start at login on.")
    } else {
        None
    }
}

fn exe() -> String {
    std::env::current_exe().and_then(|p| p.canonicalize()).map(|p| p.display().to_string()).unwrap_or_default()
}

pub fn refusal() -> Option<&'static str> {
    login_item_refusal(&exe(), cfg!(debug_assertions))
}

/// `ProgramArguments[0]` of a LaunchAgent plist as the crate writes it.
fn plist_program(plist: &str) -> Option<&str> {
    plist.split("<key>ProgramArguments</key>").nth(1)?.split("<string>").nth(1)?.split("</string>").next()
}

/// Whether to rewrite the login item at startup: its plist keeps the path from the day it was enabled, so a moved app
/// goes stale. Never an item that is off, never a refused path, and only when the recorded path differs, so a normal
/// start writes nothing (no macOS "Background Items Added" notice each time).
fn should_refresh(enabled: bool, refused: bool, recorded: Option<&str>, exe: &str) -> bool {
    enabled && !refused && recorded != Some(exe)
}

/// The OS's own answer: whether the plist exists. A toggle in System Settings › Login Items is not visible to it.
pub fn enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// At startup. `first_run`: turn the item on (plan §4.9: on by default), unless this copy is refused. Returns whether
/// the first run is done (false when refused, so the installed app still turns it on later).
pub fn startup(app: &AppHandle, first_run: bool) -> bool {
    let launcher = app.autolaunch();
    if first_run {
        if let Some(why) = refusal() {
            eprintln!("hop: start at login not turned on: {why}");
            return false;
        }
        if let Err(e) = launcher.enable() {
            eprintln!("hop: could not turn on start at login ({e})");
        }
        return true;
    }
    let exe = exe();
    let plist = std::env::var_os("HOME").and_then(|h| std::fs::read_to_string(Path::new(&h).join(format!("Library/LaunchAgents/{AUTOSTART_NAME}.plist"))).ok());
    if should_refresh(launcher.is_enabled().unwrap_or(false), refusal().is_some(), plist.as_deref().and_then(plist_program), &exe)
        && let Err(e) = launcher.enable()
    {
        eprintln!("hop: could not refresh the start at login item ({e})");
    }
    true
}

/// Flips the login item, then answers what the OS reports. Turning it off is never refused: a development build may
/// remove an item the installed app wrote.
#[tauri::command(async)]
pub fn set_login(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let verb = if enabled { "turn on" } else { "turn off" };
    if let Some(why) = refusal().filter(|_| enabled) {
        return Err(why.into());
    }
    let launcher = app.autolaunch();
    if let Err(e) = if enabled { launcher.enable() } else { launcher.disable() } {
        eprintln!("hop: could not {verb} start at login ({e})");
        return Err(format!("Could not {verb} start at login: {e}"));
    }
    let now = launcher.is_enabled().map_err(|e| e.to_string())?;
    if now != enabled {
        return Err(format!("Could not {verb} start at login: the system kept the previous state."));
    }
    Ok(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_dev_builds_translocated_and_unwritable_paths() {
        let ok = "/Applications/hop.app/Contents/MacOS/hop";
        assert_eq!(login_item_refusal(ok, false), None);
        assert!(login_item_refusal(ok, true).unwrap().contains("development build"));
        let moved = "/private/var/folders/x/T/AppTranslocation/1234/d/hop.app/Contents/MacOS/hop";
        assert!(login_item_refusal(moved, false).unwrap().contains("temporary location"));
        assert!(login_item_refusal("/Users/me/Apps & Tools/hop.app/Contents/MacOS/hop", false).unwrap().contains("& < or >"));
        // a release build run from its cargo output folder (a checkout or a worktree)
        let built = "/Volumes/Dev/hop/src-tauri/target/release/bundle/macos/hop.app/Contents/MacOS/hop";
        assert!(login_item_refusal(built, false).unwrap().contains("build folder"));
        assert_eq!(login_item_refusal("/Applications/target practice/hop.app/Contents/MacOS/hop", false), None);
        // `--target aarch64-apple-darwin` (as the release workflow builds) puts the profile one level deeper
        let cross = "/Users/me/hop/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/hop.app/Contents/MacOS/hop";
        assert!(login_item_refusal(cross, false).unwrap().contains("build folder"));
    }

    #[test]
    fn refreshes_only_an_enabled_item_whose_path_moved() {
        let plist = "<dict><key>Label</key><string>dev.hop.app</string><key>ProgramArguments</key><array><string>/Old/hop.app/Contents/MacOS/hop</string><string>--hidden</string></array></dict>";
        let rec = plist_program(plist);
        assert_eq!(rec, Some("/Old/hop.app/Contents/MacOS/hop"));
        assert!(should_refresh(true, false, rec, "/Applications/hop.app/Contents/MacOS/hop"));
        assert!(!should_refresh(true, false, rec, "/Old/hop.app/Contents/MacOS/hop"));
        assert!(!should_refresh(false, false, rec, "/New"));
        assert!(!should_refresh(true, true, rec, "/New"));
        assert_eq!(plist_program("<dict></dict>"), None);
    }

    #[test]
    fn a_second_holder_of_the_instance_lock_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(hold_instance_lock(tmp.path(), Duration::ZERO));
        let t = std::time::Instant::now();
        assert!(!hold_instance_lock(tmp.path(), Duration::from_millis(300)), "a second open of the same lock file must not take it");
        assert!(t.elapsed() >= Duration::from_millis(300), "the relaunch waits before giving up");
    }

    #[test]
    fn only_a_fresh_restart_mark_waits_and_it_is_consumed() {
        let d = tempfile::tempdir().unwrap();
        assert!(!take_restart_mark(d.path()));
        assert_eq!(lock_wait(restart_mark_fresh(d.path())), Duration::ZERO);
        mark_restart(d.path());
        assert_eq!(lock_wait(restart_mark_fresh(d.path())), Duration::from_secs(5));
        assert!(take_restart_mark(d.path()));
        assert!(!take_restart_mark(d.path()), "consumed");
        mark_restart(d.path());
        clear_restart_mark(d.path());
        assert!(!take_restart_mark(d.path()), "a failed install clears it");
        mark_restart(d.path());
        let old = std::time::SystemTime::now() - RESTART_MARK_TTL - Duration::from_secs(5);
        std::fs::File::options().write(true).open(d.path().join(RESTART_MARK)).unwrap().set_modified(old).unwrap();
        assert!(!restart_mark_fresh(d.path()), "a stale mark must not make a login start wait");
        assert!(!take_restart_mark(d.path()));
        assert!(!d.path().join(RESTART_MARK).exists());
    }

    #[test]
    fn autostart_name_is_the_bundle_identifier() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], AUTOSTART_NAME);
    }
}
