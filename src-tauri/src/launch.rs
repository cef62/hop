//! Opening what a palette row stands for with `open`: an app (a running one comes to the front, a stopped one starts)
//! or a System Settings section. A system command goes to `commands::exec`.

use std::path::Path;
use std::process::Command;

use tauri::AppHandle;

use crate::apps::{Item, Kind};

/// The `x-apple.systempreferences:` link that opens System Settings on the section `id` (`pane:<bundle id>`).
pub fn pane_url(id: &str) -> Option<String> {
    crate::panes::bundle_id(id).filter(|b| !b.is_empty()).map(|b| format!("x-apple.systempreferences:{b}"))
}

/// What `run` does for an item: `open` with `args`; on success, count a use when `record`; if `open` cannot start, say
/// `fail`.
#[derive(Debug)]
struct Open {
    args: Vec<String>,
    record: bool,
    fail: &'static str,
}

#[derive(Debug)]
enum Plan {
    Open(Open),
    /// a system command: `commands::exec`
    Command,
}

/// The plan for `item`, the index entry of the id the webview sent (`None`: not in the index). `reveal` (⌘Enter) shows
/// an app in Finder, which is not a use of it; a section has no file to show and opens as with Enter. Only an app is a
/// file that can be gone (`exists`, the filesystem in `run`).
fn open_args(item: Option<&Item>, reveal: bool, exists: impl Fn(&str) -> bool) -> Result<Plan, String> {
    let item = item.ok_or("Not something hop knows")?;
    match item.kind {
        Kind::App if !exists(&item.id) => Err("This app is no longer there".into()),
        Kind::App => {
            let args = if reveal { vec!["-R".into(), item.id.clone()] } else { vec![item.id.clone()] };
            Ok(Plan::Open(Open { args, record: !reveal, fail: "Could not open this app" }))
        }
        Kind::Pane => {
            let url = pane_url(&item.id).ok_or("Not a System Settings section")?;
            Ok(Plan::Open(Open { args: vec![url], record: true, fail: "Could not open System Settings" }))
        }
        Kind::Command => Ok(Plan::Command),
    }
}

/// Runs `act`, then counts a use only if it worked: a refused Apple Event is no use of the command.
fn counted(act: impl FnOnce() -> Result<(), String>, record: impl FnOnce()) -> Result<(), String> {
    act()?;
    record();
    Ok(())
}

/// Opens row `id` (`query`: what was typed, folded by the webview, for the pick the use teaches), then hides the
/// palette. Only items in the index run, since the id comes from the webview (a command is in it only while "System
/// commands" is on). An app removed since the last scan fails here and the palette stays
/// open with the error; a failure `open` reports later (a damaged app) is logged, and macOS shows its own alert for most
/// of those. Async, so a command's Apple Event (and the Automation prompt its first use brings) never blocks the main
/// thread; the palette is hidden on the main thread.
#[tauri::command(async)]
pub fn run(app: AppHandle, id: String, reveal: bool, query: Option<String>) -> Result<(), String> {
    // a use also teaches hop what `query` (folded by the webview) is for; no query, no pick
    let used = |app: &AppHandle| {
        crate::history::launched(app, &id);
        crate::picks::picked(app, query.as_deref().unwrap_or(""), &id);
    };
    let open = match open_args(crate::apps::item(&app, &id).as_ref(), reveal, |p| Path::new(p).exists())? {
        Plan::Command => return counted(|| crate::commands::exec(&id, &mut crate::commands::System(app.clone())), || used(&app)),
        Plan::Open(open) => open,
    };
    let mut child = Command::new("/usr/bin/open").args(&open.args).spawn().map_err(|e| {
        eprintln!("hop: could not run open for {id} ({e})");
        open.fail.to_string()
    })?;
    crate::panel::on_main(&app, crate::panel::hide);
    if open.record {
        used(&app);
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

    /// The `open` plan, or an error (a command is no `open`).
    fn opens(item: Option<&Item>, reveal: bool, exists: impl Fn(&str) -> bool) -> Result<Open, String> {
        match open_args(item, reveal, exists)? {
            Plan::Open(o) => Ok(o),
            Plan::Command => Err("a command".into()),
        }
    }

    fn item(kind: Kind, id: &str) -> Item {
        Item { kind, id: id.into(), name: "X".into(), alt_names: vec![], path: Some("/System/Applications/System Settings.app".into()), confirm: None }
    }

    #[test]
    fn only_indexed_items_run_and_each_kind_opens_its_own_way() {
        let there = |_: &str| true;
        let refused = |r: Result<Open, String>| r.unwrap_err();
        assert_eq!(refused(opens(None, false, there)), "Not something hop knows");
        let a = Item { path: Some("/A.app".into()), ..item(Kind::App, "/A.app") };
        let args = |o: Open| o.args;
        assert_eq!(args(opens(Some(&a), false, there).unwrap()), ["/A.app"]);
        assert_eq!(args(opens(Some(&a), true, there).unwrap()), ["-R", "/A.app"]);
        // a section opens System Settings on it; ⌘Enter has nothing to reveal and opens it too
        let p = item(Kind::Pane, "pane:com.apple.settings.Storage");
        for reveal in [false, true] {
            assert_eq!(args(opens(Some(&p), reveal, there).unwrap()), ["x-apple.systempreferences:com.apple.settings.Storage"]);
        }
        assert!(opens(Some(&item(Kind::Pane, "com.apple.settings.Storage")), false, there).is_err());
        // a command goes to `commands::exec`, ⌘Enter or not
        for reveal in [false, true] {
            assert!(matches!(open_args(Some(&item(Kind::Command, "cmd:lock")), reveal, there), Ok(Plan::Command)));
        }
    }

    #[test]
    fn only_apps_need_their_bundle_and_only_a_reveal_is_not_a_use() {
        let gone = |_: &str| false;
        let a = Item { path: Some("/A.app".into()), ..item(Kind::App, "/A.app") };
        assert_eq!(opens(Some(&a), false, gone).unwrap_err(), "This app is no longer there");
        // a section id is no file: it opens whatever `exists` says
        let p = item(Kind::Pane, "pane:com.apple.settings.Storage");
        let o = opens(Some(&p), false, gone).unwrap();
        assert!(o.record && !o.fail.contains("app"), "{}", o.fail);
        assert!(opens(Some(&p), true, gone).unwrap().record, "⌘Enter on a section is a use");
        let there = |_: &str| true;
        assert!(opens(Some(&a), false, there).unwrap().record);
        assert!(!opens(Some(&a), true, there).unwrap().record, "revealing in Finder counted as a use");
        assert_eq!(opens(Some(&a), false, there).unwrap().fail, "Could not open this app");
    }

    #[test]
    fn a_command_counts_as_a_use_only_once_it_ran() {
        let log = std::cell::RefCell::new(vec![]);
        let act = |r: Result<(), String>| {
            let log = &log;
            move || {
                log.borrow_mut().push("run");
                r
            }
        };
        let record = || log.borrow_mut().push("record");
        assert_eq!(counted(act(Err("Could not restart".into())), record), Err("Could not restart".into()));
        assert_eq!(*log.borrow(), ["run"]);
        assert_eq!(counted(act(Ok(())), record), Ok(()));
        assert_eq!(*log.borrow(), ["run", "run", "record"]);
    }

    #[test]
    fn sections_open_by_bundle_id_and_nothing_else_does() {
        assert_eq!(pane_url("pane:com.apple.settings.Storage").as_deref(), Some("x-apple.systempreferences:com.apple.settings.Storage"));
        assert_eq!(pane_url("/Applications/X.app"), None);
        assert_eq!(pane_url("pane:"), None);
    }
}
