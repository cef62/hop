//! The last calculator results the user copied, newest first, in `calc_history.json` (app data dir). Written on every
//! record (a handful of short strings, so no debounce). A missing or corrupt file starts empty.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// In the app data dir, next to the launch history (`history::FILE`), which it must never overwrite.
pub const FILE: &str = "calc_history.json";
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

/// An expression without its whitespace: `2+2` and `2 + 2` are one sum.
fn bare(expr: &str) -> String {
    expr.split_whitespace().collect()
}

/// The entry for a copied `result` of `expr`, or none when it is not a plausible sum.
fn entry(expr: &str, result: &str) -> Option<Entry> {
    Some(Entry { expr: expr.trim().into(), result: result.into() }).filter(valid)
}

/// `e` first; an earlier entry with the same expression (whitespace aside) is replaced, not repeated: the latest
/// spacing is the one kept.
pub fn record(list: &mut Vec<Entry>, e: Entry) {
    let key = bare(&e.expr);
    list.retain(|x| bare(&x.expr) != key);
    list.insert(0, e);
    list.truncate(KEEP);
}

pub fn load(file: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut list: Vec<Entry> = serde_json::from_str(&text).unwrap_or_default();
    list.retain(valid);
    // a hand-edited file: the first (newest) of a repeated expression stays, as `record` would have it
    let mut seen = std::collections::HashSet::new();
    list.retain(|e| seen.insert(bare(&e.expr)));
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
    let file = app.path().app_data_dir().ok().map(|d| d.join(FILE));
    let list = file.as_deref().map(load).unwrap_or_default();
    app.manage(Mutex::new(Store { list, file }));
}

/// Records a copied result. An entry that is not a plausible sum is dropped; a failed save is logged and the entry
/// stays in memory, so the copy that triggered it never fails.
pub fn push_to(store: &mut Store, expr: &str, result: &str) {
    let Some(e) = entry(expr, result) else { return };
    record(&mut store.list, e);
    if let Some(f) = &store.file
        && let Err(err) = save(f, &store.list)
    {
        eprintln!("hop: could not save {} ({err})", f.display());
    }
}

pub fn push(app: &AppHandle, expr: &str, result: &str) {
    push_to(&mut app.state::<Mutex<Store>>().lock().unwrap(), expr, result);
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
        let many: Vec<Entry> = [e(""), e("   "), e(&"1+".repeat(101))].into_iter().chain((0..30).map(|i| e(&format!("{i}+1")))).collect();
        save(&file, &many).unwrap();
        assert_eq!(load(&file).len(), KEEP);
        assert_eq!(load(&file)[0].expr, "0+1");
        save(&file, &[e("1+1"), e("2+2"), Entry { expr: "1+1".into(), result: "9".into() }]).unwrap();
        assert_eq!(load(&file), [e("1+1"), e("2+2")]);
    }

    #[test]
    fn whitespace_variants_are_one_entry_and_the_latest_spacing_wins() {
        let mut l = vec![];
        record(&mut l, e("2+2"));
        record(&mut l, e("3+3"));
        record(&mut l, e("2 + 2"));
        assert_eq!(l.iter().map(|x| x.expr.as_str()).collect::<Vec<_>>(), ["2 + 2", "3+3"]);
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(FILE);
        save(&file, &[e("2 + 2"), e("3+3"), e("2+2"), e("2\t+ 2")]).unwrap();
        assert_eq!(load(&file), [e("2 + 2"), e("3+3")]);
    }

    #[test]
    fn push_saves_the_file_a_restart_reads_back() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(FILE);
        let mut store = Store { list: vec![], file: Some(file.clone()) };
        push_to(&mut store, " 2+2 ", "4");
        push_to(&mut store, "", "4"); // not plausible: nothing
        assert_eq!(load(&file), [Entry { expr: "2+2".into(), result: "4".into() }]);
        assert_eq!(store.list.len(), 1);
        // an unwritable place does not lose the entry in memory, and does not panic
        let blocked = tmp.path().join(FILE).join("sub/x.json"); // its parent is a file
        let mut store = Store { list: vec![], file: Some(blocked) };
        push_to(&mut store, "1+1", "2");
        assert_eq!(store.list.len(), 1);
    }

    #[test]
    fn the_file_is_not_the_launch_history() {
        assert_eq!(FILE, "calc_history.json");
        assert_ne!(FILE, crate::history::FILE);
    }

    #[test]
    fn a_copy_makes_a_trimmed_entry_or_none() {
        assert_eq!(entry(" 2 + 2 ", "4"), Some(Entry { expr: "2 + 2".into(), result: "4".into() }));
        assert_eq!(entry("  ", "4"), None);
        assert_eq!(entry("2+2", ""), None);
        assert_eq!(entry(&"1".repeat(MAX_CHARS + 1), "4"), None);
    }

    #[test]
    fn only_plausible_entries_are_valid() {
        assert!(valid(&e("1+1")) && valid(&e(&"1".repeat(MAX_CHARS))));
        assert!(!valid(&e("")) && !valid(&e("  ")) && !valid(&e(&"1".repeat(MAX_CHARS + 1))));
        assert!(!valid(&Entry { expr: "1+1".into(), result: "".into() }));
    }
}
