//! Launching and revealing apps with `open`: a running app comes to the front, a stopped one starts.

use std::path::Path;
use std::process::Command;

use tauri::AppHandle;

/// `open <path>` (or `open -R <path>`: reveal in Finder), then hide the palette. Only apps in the index are opened, since
/// the path comes from the webview. An app removed since the last scan fails here and the palette stays open with the
/// error; a failure `open` reports later (a damaged app) is logged, and macOS shows its own alert for most of those.
#[tauri::command]
pub fn launch(app: AppHandle, path: String, reveal: bool) -> Result<(), String> {
    if !crate::apps::known(&app, &path) {
        return Err("Not an app hop knows".into());
    }
    if !Path::new(&path).exists() {
        return Err("This app is no longer there".into());
    }
    let mut cmd = Command::new("/usr/bin/open");
    if reveal {
        cmd.arg("-R");
    }
    let mut child = cmd.arg(&path).spawn().map_err(|e| {
        eprintln!("hop: could not run open for {path} ({e})");
        "Could not open this app".to_string()
    })?;
    crate::panel::hide(&app);
    if !reveal {
        crate::history::launched(&app, &path);
    }
    std::thread::spawn(move || match child.wait() {
        Ok(s) if s.success() => {}
        Ok(s) => eprintln!("hop: open {path} failed ({s})"),
        Err(e) => eprintln!("hop: open {path} failed ({e})"),
    });
    Ok(())
}
