//! The last calculator results the user copied, newest first, in `calc_history.json` (app data dir). Written on every
//! record (a handful of short strings, so no debounce). A missing or corrupt file starts empty.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Stored entries.
pub const KEEP: usize = 20;
/// Longest expression kept: the palette's calculator takes no more (`calc.ts`), so a longer one is never a sum.
pub const MAX_CHARS: usize = 200;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub expr: String,
    pub result: String,
}

fn valid(e: &Entry) -> bool {
    !e.expr.trim().is_empty() && e.expr.chars().count() <= MAX_CHARS && !e.result.is_empty()
}

/// `e` first; an earlier entry with the same expression is replaced, not repeated.
pub fn record(list: &mut Vec<Entry>, e: Entry) {
    list.retain(|x| x.expr != e.expr);
    list.insert(0, e);
    list.truncate(KEEP);
}

pub fn load(file: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut list: Vec<Entry> = serde_json::from_str(&text).unwrap_or_default();
    list.retain(valid);
    list.truncate(KEEP);
    list
}

/// Atomic: a crash mid-write leaves the old file, never half a file.
pub fn save(file: &Path, list: &[Entry]) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(list).map_err(std::io::Error::other)?)?;
    std::fs::rename(&tmp, file)
}

pub struct Store {
    list: Vec<Entry>,
    file: Option<PathBuf>,
}

pub fn setup(app: &AppHandle) {
    let file = app.path().app_data_dir().ok().map(|d| d.join("calc_history.json"));
    let list = file.as_deref().map(load).unwrap_or_default();
    app.manage(Mutex::new(Store { list, file }));
}

/// Records a copied result. An entry that is not a plausible sum is dropped; a failed save is logged and the entry
/// stays in memory, so the copy that triggered it never fails.
pub fn push(app: &AppHandle, expr: &str, result: &str) {
    let e = Entry { expr: expr.trim().into(), result: result.into() };
    if !valid(&e) {
        return;
    }
    let state = app.state::<Mutex<Store>>();
    let mut s = state.lock().unwrap();
    record(&mut s.list, e);
    if let Some(f) = &s.file
        && let Err(err) = save(f, &s.list)
    {
        eprintln!("hop: could not save {} ({err})", f.display());
    }
}

/// For the palette's history rows.
#[tauri::command]
pub fn calc_history(app: AppHandle) -> Vec<Entry> {
    app.state::<Mutex<Store>>().lock().unwrap().list.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(expr: &str) -> Entry {
        Entry { expr: expr.into(), result: "1".into() }
    }

    #[test]
    fn record_moves_a_repeat_to_the_top() {
        let mut l = vec![];
        for x in ["1+1", "2+2", "3+3"] {
            record(&mut l, e(x));
        }
        record(&mut l, Entry { expr: "1+1".into(), result: "new".into() });
        assert_eq!(l.iter().map(|x| x.expr.as_str()).collect::<Vec<_>>(), ["1+1", "3+3", "2+2"]);
        assert_eq!(l[0].result, "new");
    }

    #[test]
    fn record_keeps_twenty() {
        let mut l = vec![];
        for i in 0..25 {
            record(&mut l, e(&format!("{i}+1")));
        }
        assert_eq!(l.len(), KEEP);
        assert_eq!((l[0].expr.as_str(), l[KEEP - 1].expr.as_str()), ("24+1", "5+1"));
    }

    #[test]
    fn corrupt_file_reads_empty_and_save_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("sub/calc_history.json");
        assert!(load(&file).is_empty());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        for bad in ["{not json", r#"{"expr":"1+1"}"#, r#"[{"expr":"1+1"}]"#, ""] {
            std::fs::write(&file, bad).unwrap();
            assert!(load(&file).is_empty(), "{bad}");
        }
        let l = vec![e("1+1"), e("2+2")];
        save(&file, &l).unwrap();
        assert_eq!(load(&file), l);
        assert!(!file.with_extension("json.tmp").exists());
        // a hand-edited file: junk entries dropped, the rest capped at KEEP
        let many: Vec<Entry> = (0..30).map(|i| e(&format!("{i}+1"))).chain([e(""), e("   "), e(&"1+".repeat(101))]).collect();
        save(&file, &many).unwrap();
        assert_eq!(load(&file).len(), KEEP);
    }

    #[test]
    fn only_plausible_entries_are_valid() {
        assert!(valid(&e("1+1")) && valid(&e(&"1".repeat(MAX_CHARS))));
        assert!(!valid(&e("")) && !valid(&e("  ")) && !valid(&e(&"1".repeat(MAX_CHARS + 1))));
        assert!(!valid(&Entry { expr: "1+1".into(), result: "".into() }));
    }
}
