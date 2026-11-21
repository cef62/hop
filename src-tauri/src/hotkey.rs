//! The global hotkey. Fixed to the default until settings land (M3); a failed registration leaves hop running, and
//! relaunching it (`RunEvent::Reopen`) still shows the palette.

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const DEFAULT: &str = "Alt+Space";

pub fn register(app: &AppHandle, started: std::time::Instant) {
    let res = DEFAULT.parse::<Shortcut>().map_err(|e| e.to_string()).and_then(|sc| {
        app.global_shortcut()
            .on_shortcut(sc, |app, _, ev| {
                if ev.state == ShortcutState::Pressed {
                    crate::panel::toggle(app);
                }
            })
            .map_err(|e| e.to_string())
    });
    match res {
        Ok(()) => eprintln!("hop: {DEFAULT} registered {} ms after start", started.elapsed().as_millis()),
        // shortcut: stderr only, until M3's tray and Settings show the conflict
        Err(e) => eprintln!("hop: could not register the {DEFAULT} hotkey ({e}); relaunch hop to open the palette"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_parses() {
        assert!(super::DEFAULT.parse::<tauri_plugin_global_shortcut::Shortcut>().is_ok());
    }
}
