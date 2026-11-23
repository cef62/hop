//! Learned picks: which item the user opened for which query, in `picks.json` (app data dir), so "me" can list Meet
//! first. Scores add 1 per pick and halve every week like frecency (`history`), whose `Entry` and `decayed` this
//! reuses; `history.json` is never touched. The frontend sends the query already folded (`pickKey` in `src/rank.ts`),
//! so folding lives in one place; this side only caps the length. A missing or corrupt file starts empty.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::history::{Entry, decayed, now};

/// Query → item id → score.
pub type Picks = HashMap<String, HashMap<String, Entry>>;

/// In the app data dir, next to `history::FILE`, which it must never overwrite.
pub const FILE: &str = "picks.json";
/// Characters of a query that are learned; `PICK_KEY` in `src/rank.ts`.
const KEY_MAX: usize = 30;
const MAX_KEYS: usize = 500;
/// Same as `history`: a pick that decayed below this is dropped on prune.
const FORGET: f64 = 0.01;
const DEBOUNCE: Duration = Duration::from_secs(2);

/// Counts one pick of `id` for `key` (trimmed, cut at `KEY_MAX` characters). Nothing for an empty key or id: the empty
/// query has no pick to learn.
pub fn record_pick(picks: &mut Picks, key: &str, id: &str, now: u64) {
    let key: String = key.trim().chars().take(KEY_MAX).collect();
    if key.is_empty() || id.is_empty() {
        return;
    }
    let ids = picks.entry(key).or_default();
    let score = ids.get(id).map_or(0.0, |e| decayed(e, now)) + 1.0;
    ids.insert(id.to_string(), Entry { score, at: now });
    prune(picks, now);
}

/// Drops decayed-out picks and empty keys, then the stalest keys beyond `MAX_KEYS` (stalest: oldest newest-pick).
pub fn prune(picks: &mut Picks, now: u64) {
    for ids in picks.values_mut() {
        ids.retain(|_, e| decayed(e, now) >= FORGET);
    }
    picks.retain(|_, ids| !ids.is_empty());
    if picks.len() > MAX_KEYS {
        let last = |ids: &HashMap<String, Entry>| ids.values().map(|e| e.at).max().unwrap_or(0);
        let mut keys: Vec<(u64, String)> = picks.iter().map(|(k, ids)| (last(ids), k.clone())).collect();
        keys.sort(); // oldest first, key name breaks ties
        for (_, k) in keys.into_iter().take(picks.len() - MAX_KEYS) {
            picks.remove(&k);
        }
    }
}

/// Decayed scores for the frontend.
pub fn scores(picks: &Picks, now: u64) -> HashMap<String, HashMap<String, f64>> {
    picks.iter().map(|(k, ids)| (k.clone(), ids.iter().map(|(id, e)| (id.clone(), decayed(e, now))).collect())).collect()
}

pub fn load(file: &Path) -> Picks {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut picks: Picks = serde_json::from_str(&text).unwrap_or_default();
    for ids in picks.values_mut() {
        ids.retain(|_, e| e.score.is_finite() && e.score > 0.0);
    }
    picks.retain(|k, ids| !ids.is_empty() && !k.is_empty());
    picks
}

/// Atomic like `history::save`: a crash mid-write leaves the old file.
pub fn save(file: &Path, picks: &Picks, now: u64) -> std::io::Result<()> {
    let mut kept = picks.clone();
    prune(&mut kept, now);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&kept).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, file)
}

pub struct Store {
    picks: Picks,
    file: Option<PathBuf>,
    dirty: bool,
}

pub fn setup(app: &AppHandle) {
    let file = app.path().app_data_dir().ok().map(|d| d.join(FILE));
    let picks = file.as_deref().map(load).unwrap_or_default();
    app.manage(Mutex::new(Store { picks, file, dirty: false }));
}

fn flush(s: &mut Store) {
    if let (true, Some(file)) = (s.dirty, &s.file)
        && let Err(e) = save(file, &s.picks, now())
    {
        eprintln!("hop: could not save {} ({e})", file.display());
    }
    s.dirty = false;
}

/// A pick of `id` for `key`, counted now and saved within `DEBOUNCE`.
pub fn picked(app: &AppHandle, key: &str, id: &str) {
    let state = app.state::<Mutex<Store>>();
    let mut s = state.lock().unwrap();
    record_pick(&mut s.picks, key, id, now());
    if !std::mem::replace(&mut s.dirty, true) {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(DEBOUNCE);
            flush(&mut app.state::<Mutex<Store>>().lock().unwrap());
        });
    }
}

/// On exit: a pick within the last `DEBOUNCE` is not lost.
pub fn flush_now(app: &AppHandle) {
    if let Some(state) = app.try_state::<Mutex<Store>>() {
        flush(&mut state.lock().unwrap());
    }
}

/// Current pick scores, for ranking.
#[tauri::command]
pub fn picks(app: AppHandle) -> HashMap<String, HashMap<String, f64>> {
    scores(&app.state::<Mutex<Store>>().lock().unwrap().picks, now())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEK: u64 = 7 * 24 * 3600;

    #[test]
    fn picks_add_up_and_halve_each_week_like_history() {
        let mut p = Picks::new();
        record_pick(&mut p, "me", "/Meet.app", 1000);
        record_pick(&mut p, "me", "/Meet.app", 1000);
        assert_eq!(scores(&p, 1000)["me"]["/Meet.app"], 2.0);
        assert!((scores(&p, 1000 + WEEK)["me"]["/Meet.app"] - 1.0).abs() < 1e-9);
        record_pick(&mut p, "me", "/Meet.app", 1000 + 2 * WEEK); // 0.5 left, plus this pick
        assert!((p["me"]["/Meet.app"].score - 1.5).abs() < 1e-9);
        // the same decay as history.rs
        let e = p["me"]["/Meet.app"];
        assert_eq!(decayed(&e, 5 * WEEK), crate::history::decayed(&e, 5 * WEEK));
        // another item under the same key and the same item under another key are counted apart
        record_pick(&mut p, "me", "/Mail.app", 0);
        record_pick(&mut p, "mee", "/Meet.app", 0);
        assert_eq!(p["me"].len(), 2);
        assert_eq!(p["mee"]["/Meet.app"].score, 1.0);
    }

    #[test]
    fn keys_are_trimmed_capped_at_30_chars_and_never_empty() {
        let mut p = Picks::new();
        record_pick(&mut p, "  me ", "/Meet.app", 0);
        assert!(p.contains_key("me"));
        let long = "é".repeat(45); // characters, not bytes
        record_pick(&mut p, &long, "/A.app", 0);
        assert_eq!(p.keys().find(|k| k.starts_with('é')).unwrap().chars().count(), 30);
        for key in ["", "   "] {
            record_pick(&mut p, key, "/A.app", 0);
        }
        record_pick(&mut p, "x", "", 0);
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn prune_drops_decayed_picks_empty_keys_and_the_stalest_keys_beyond_500() {
        let mut p = Picks::new();
        p.entry("old".into()).or_default().insert("/A.app".into(), Entry { score: 0.001, at: 0 });
        p.entry("mixed".into()).or_default().insert("/A.app".into(), Entry { score: 0.001, at: 0 });
        p.get_mut("mixed").unwrap().insert("/B.app".into(), Entry { score: 1.0, at: 0 });
        prune(&mut p, 0);
        assert!(!p.contains_key("old") && p["mixed"].len() == 1);

        let mut p = Picks::new();
        for n in 0..MAX_KEYS + 3 {
            record_pick(&mut p, &format!("k{n}"), "/A.app", n as u64); // k0, k1, k2 are the stalest
        }
        assert_eq!(p.len(), MAX_KEYS);
        assert!(["k0", "k1", "k2"].iter().all(|k| !p.contains_key(*k)) && p.contains_key("k3") && p.contains_key("k502"));
        // a key refreshed by a new pick is not stale
        let mut p = Picks::new();
        for n in 0..MAX_KEYS {
            record_pick(&mut p, &format!("k{n}"), "/A.app", n as u64);
        }
        record_pick(&mut p, "k0", "/A.app", 1000);
        record_pick(&mut p, "new", "/A.app", 1001);
        assert!(p.contains_key("k0") && !p.contains_key("k1") && p.len() == MAX_KEYS);
    }

    #[test]
    fn corrupt_or_missing_file_is_empty_and_save_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("sub/picks.json");
        assert!(load(&file).is_empty());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        for bad in ["{not json", r#"{"me":"x"}"#, r#"["a"]"#] {
            std::fs::write(&file, bad).unwrap();
            assert!(load(&file).is_empty(), "{bad}");
        }
        std::fs::write(&file, r#"{"me":{"/A.app":{"score":-1,"at":0},"/B.app":{"score":2,"at":0}},"gone":{"/A.app":{"score":0,"at":0}}}"#).unwrap();
        assert_eq!(load(&file).keys().collect::<Vec<_>>(), ["me"]);
        assert_eq!(load(&file)["me"].keys().collect::<Vec<_>>(), ["/B.app"]);

        let mut p = Picks::new();
        record_pick(&mut p, "me", "/A.app", 0);
        p.entry("old".into()).or_default().insert("/A.app".into(), Entry { score: 0.001, at: 0 });
        save(&file, &p, 0).unwrap();
        assert_eq!(load(&file), Picks::from([("me".into(), HashMap::from([("/A.app".into(), Entry { score: 1.0, at: 0 })]))]));
        assert!(!file.with_extension("json.tmp").exists());
    }

    #[test]
    fn the_file_is_not_the_launch_history_or_calculator_history() {
        assert_eq!(FILE, "picks.json");
        assert_ne!(FILE, crate::history::FILE);
        assert_ne!(FILE, crate::calc_history::FILE);
    }
}
