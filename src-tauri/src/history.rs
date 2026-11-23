//! Frecency: a launch count per item id (an app's path) that halves every week, in `history.json` (app data dir). Written after a
//! launch, at most once per `DEBOUNCE`, and on exit. A missing or corrupt file starts an empty history.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const HALF_LIFE_SECS: f64 = 7.0 * 24.0 * 3600.0;
/// Entries that decayed below this are dropped on save, so the file does not grow with every app ever launched.
const FORGET: f64 = 0.01;
const DEBOUNCE: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    /// Decayed launch count as of `at`.
    pub score: f64,
    /// Unix seconds of the last launch.
    pub at: u64,
}

pub type Entries = HashMap<String, Entry>;

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn decayed(e: &Entry, now: u64) -> f64 {
    e.score * 0.5f64.powf(now.saturating_sub(e.at) as f64 / HALF_LIFE_SECS)
}

pub fn record(entries: &mut Entries, path: &str, now: u64) {
    let score = entries.get(path).map_or(0.0, |e| decayed(e, now)) + 1.0;
    entries.insert(path.to_string(), Entry { score, at: now });
}

pub fn scores(entries: &Entries, now: u64) -> HashMap<String, f64> {
    entries.iter().map(|(p, e)| (p.clone(), decayed(e, now))).collect()
}

pub fn load(file: &Path) -> Entries {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut entries: Entries = serde_json::from_str(&text).unwrap_or_default();
    entries.retain(|_, e| e.score.is_finite() && e.score > 0.0);
    entries
}

/// Atomic: a crash mid-write leaves the old file, never half a file.
pub fn save(file: &Path, entries: &Entries, now: u64) -> std::io::Result<()> {
    let kept: Entries = entries.iter().filter(|(_, e)| decayed(e, now) >= FORGET).map(|(p, e)| (p.clone(), *e)).collect();
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&kept).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, file)
}

pub struct History {
    entries: Entries,
    file: Option<PathBuf>,
    /// A save is scheduled or the entries changed since the last one.
    dirty: bool,
}

pub fn setup(app: &AppHandle) {
    let file = app.path().app_data_dir().ok().map(|d| d.join("history.json"));
    let entries = file.as_deref().map(load).unwrap_or_default();
    app.manage(Mutex::new(History { entries, file, dirty: false }));
}

fn flush(h: &mut History) {
    if let (true, Some(file)) = (h.dirty, &h.file)
        && let Err(e) = save(file, &h.entries, now())
    {
        eprintln!("hop: could not save {} ({e})", file.display());
    }
    h.dirty = false;
}

/// A launch of item `id`: counted now, saved within `DEBOUNCE`.
pub fn launched(app: &AppHandle, id: &str) {
    let state = app.state::<Mutex<History>>();
    let mut h = state.lock().unwrap();
    record(&mut h.entries, id, now());
    if !std::mem::replace(&mut h.dirty, true) {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(DEBOUNCE);
            flush(&mut app.state::<Mutex<History>>().lock().unwrap());
        });
    }
}

/// On exit: a launch within the last `DEBOUNCE` is not lost.
pub fn flush_now(app: &AppHandle) {
    if let Some(state) = app.try_state::<Mutex<History>>() {
        flush(&mut state.lock().unwrap());
    }
}

/// Current frecency per item id, for ranking and the empty-query list.
#[tauri::command]
pub fn frecency(app: AppHandle) -> HashMap<String, f64> {
    scores(&app.state::<Mutex<History>>().lock().unwrap().entries, now())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEK: u64 = 7 * 24 * 3600;

    #[test]
    fn launches_add_up_and_halve_each_week() {
        let mut h = Entries::new();
        record(&mut h, "/A.app", 1000);
        record(&mut h, "/A.app", 1000);
        assert_eq!(scores(&h, 1000)["/A.app"], 2.0);
        assert!((scores(&h, 1000 + WEEK)["/A.app"] - 1.0).abs() < 1e-9);
        record(&mut h, "/A.app", 1000 + 2 * WEEK); // 0.5 left, plus this launch
        assert!((h["/A.app"].score - 1.5).abs() < 1e-9);
        // a clock that went backwards does not inflate the score
        assert_eq!(decayed(&Entry { score: 1.0, at: 5000 }, 10), 1.0);
    }

    #[test]
    fn corrupt_or_missing_file_is_an_empty_history_and_save_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("sub/history.json");
        assert!(load(&file).is_empty());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{not json").unwrap();
        assert!(load(&file).is_empty());
        std::fs::write(&file, r#"{"/A.app":{"score":-1,"at":0},"/B.app":{"score":2,"at":0}}"#).unwrap();
        assert_eq!(load(&file).keys().collect::<Vec<_>>(), ["/B.app"]);

        let mut h = Entries::new();
        record(&mut h, "/A.app", 0);
        record(&mut h, "/Old.app", 0);
        h.get_mut("/Old.app").unwrap().score = 0.001; // decayed past FORGET: dropped
        save(&file, &h, 0).unwrap();
        assert_eq!(load(&file), Entries::from([("/A.app".into(), Entry { score: 1.0, at: 0 })]));
        assert!(!file.with_extension("json.tmp").exists());
    }
}
