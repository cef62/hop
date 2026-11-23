//! Opening what a palette row stands for with `open`: an app (a running one comes to the front, a stopped one starts)
//! or a System Settings section.

use std::path::Path;
use std::process::Command;

use tauri::AppHandle;

use crate::apps::{Item, Kind};

/// The `x-apple.systempreferences:` link that opens System Settings on the section `id` (`pane:<bundle id>`).
pub fn pane_url(id: &str) -> Option<String> {
    crate::panes::bundle_id(id).filter(|b| !b.is_empty()).map(|b| format!("x-apple.systempreferences:{b}"))
}

/// The arguments to `open` for `item`, the index entry of the id the webview sent (`None`: not in the index).
/// `reveal` (⌘Enter) shows an app in Finder; a section has no file to show and opens as with Enter.
fn open_args(item: Option<&Item>, reveal: bool) -> Result<Vec<String>, String> {
    let item = item.ok_or("Not something hop knows")?;
    match item.kind {
        Kind::App if reveal => Ok(vec!["-R".into(), item.id.clone()]),
        Kind::App => Ok(vec![item.id.clone()]),
        Kind::Pane => pane_url(&item.id).map(|u| vec![u]).ok_or_else(|| "Not a System Settings section".into()),
        Kind::Command => Err("hop cannot run this yet".into()),
    }
}

/// Opens row `id`, then hides the palette. Only items in the index run, since the id comes from the webview. An app
/// removed since the last scan fails here and the palette stays open with the error; a failure `open` reports later
/// (a damaged app) is logged, and macOS shows its own alert for most of those.
#[tauri::command]
pub fn run(app: AppHandle, id: String, reveal: bool) -> Result<(), String> {
    let item = crate::apps::item(&app, &id);
    let args = open_args(item.as_ref(), reveal)?;
    let is_app = item.is_some_and(|i| i.kind == Kind::App);
    if is_app && !Path::new(&id).exists() {
        return Err("This app is no longer there".into());
    }
    let mut child = Command::new("/usr/bin/open").args(&args).spawn().map_err(|e| {
        eprintln!("hop: could not run open for {id} ({e})");
        if is_app { "Could not open this app" } else { "Could not open System Settings" }.to_string()
    })?;
    crate::panel::hide(&app);
    // revealing in Finder is not a use of the app
    if !(is_app && reveal) {
        crate::history::launched(&app, &id);
    }
    std::thread::spawn(move || match child.wait() {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("hop: open {id} failed ({s})"),
        Err(e) => eprintln!("hop: open {id} failed ({e})"),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: Kind, id: &str) -> Item {
        Item { kind, id: id.into(), name: "X".into(), alt_names: vec![], path: Some("/System/Applications/System Settings.app".into()), confirm: None }
    }

    #[test]
    fn only_indexed_items_run_and_each_kind_opens_its_own_way() {
        let refused = |r: Result<Vec<String>, String>| r.unwrap_err();
        assert_eq!(refused(open_args(None, false)), "Not something hop knows");
        let a = Item { path: Some("/A.app".into()), ..item(Kind::App, "/A.app") };
        assert_eq!(open_args(Some(&a), false).unwrap(), ["/A.app"]);
        assert_eq!(open_args(Some(&a), true).unwrap(), ["-R", "/A.app"]);
        // a section opens System Settings on it; ⌘Enter has nothing to reveal and opens it too
        let p = item(Kind::Pane, "pane:com.apple.settings.Storage");
        for reveal in [false, true] {
            assert_eq!(open_args(Some(&p), reveal).unwrap(), ["x-apple.systempreferences:com.apple.settings.Storage"]);
        }
        assert!(open_args(Some(&item(Kind::Pane, "com.apple.settings.Storage")), false).is_err());
        assert!(open_args(Some(&item(Kind::Command, "cmd:lock")), false).is_err());
    }

    #[test]
    fn sections_open_by_bundle_id_and_nothing_else_does() {
        assert_eq!(pane_url("pane:com.apple.settings.Storage").as_deref(), Some("x-apple.systempreferences:com.apple.settings.Storage"));
        assert_eq!(pane_url("/Applications/X.app"), None);
        assert_eq!(pane_url("pane:"), None);
    }
}
