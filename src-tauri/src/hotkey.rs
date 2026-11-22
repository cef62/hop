//! The global hotkey, from settings. A chord that does not register (the OS refuses it, or it is not valid) is logged
//! and shown in Settings; hop keeps running and the menu-bar icon stays the way in. A chord another app also holds
//! registers fine (macOS does not report it), so Settings always shows a hint about that instead (plan §4.8).

use std::sync::Mutex;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub const DEFAULT: &str = "Alt+Space";

/// Why the saved hotkey is not registered, if it is not.
static ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Empty (no hotkey) or a chord the global-shortcut plugin parses.
pub fn valid(chord: &str) -> bool {
    chord.is_empty() || chord.parse::<Shortcut>().is_ok()
}

fn register(app: &AppHandle, chord: &str) -> Result<(), String> {
    if chord.is_empty() {
        return Ok(());
    }
    let sc = chord.parse::<Shortcut>().map_err(|e| format!("{chord} is not a valid shortcut ({e})"))?;
    app.global_shortcut()
        .on_shortcut(sc, |app, _, ev| {
            if ev.state == ShortcutState::Pressed {
                crate::panel::toggle(app);
            }
        })
        .map_err(|e| format!("macOS refused {chord} ({e})"))
}

fn unregister(app: &AppHandle, chord: &str) {
    if let Ok(sc) = chord.parse::<Shortcut>() {
        app.global_shortcut().unregister(sc).ok();
    }
}

/// At startup: registers the saved chord, or records why not.
pub fn start(app: &AppHandle, chord: &str, started: std::time::Instant) {
    match register(app, chord) {
        Ok(()) => eprintln!("hop: hotkey {chord:?} registered {} ms after start", started.elapsed().as_millis()),
        Err(e) => {
            eprintln!("hop: hotkey not registered: {e}; use the menu-bar icon or relaunch hop to open the palette");
            *ERROR.lock().unwrap() = Some(e);
        }
    }
}

/// From Settings: `old` is released, `new` registered. If `new` fails, `old` is registered again and the error returned.
pub fn change(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    let (res, err) = swap(old, new, |c| register(app, c), |c| unregister(app, c));
    if let Err(e) = &res {
        eprintln!("hop: hotkey not changed: {e}");
    }
    if let Some(a) = &err {
        eprintln!("hop: hotkey not registered: {a}");
    }
    *ERROR.lock().unwrap() = err;
    res
}

/// `change` without the plugin: the result, and the error to show afterwards (none when a chord is live, or the
/// previous one could not be put back).
fn swap(old: &str, new: &str, mut reg: impl FnMut(&str) -> Result<(), String>, unreg: impl FnOnce(&str)) -> (Result<(), String>, Option<String>) {
    unreg(old);
    match reg(new) {
        Ok(()) => (Ok(()), None),
        Err(e) => {
            let again = reg(old).err();
            (Err(e), again)
        }
    }
}

pub fn error() -> Option<String> {
    ERROR.lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::{swap, valid};

    #[test]
    fn a_refused_chord_puts_the_old_one_back() {
        let log = std::cell::RefCell::new(vec![]);
        let reg = |c: &str| {
            log.borrow_mut().push(format!("+{c}"));
            if c == "Super+Space" { Err("refused".to_string()) } else { Ok(()) }
        };
        let unreg = |c: &str| log.borrow_mut().push(format!("-{c}"));
        assert_eq!(swap("Alt+Space", "Control+KeyK", reg, unreg), (Ok(()), None));
        assert_eq!(swap("Alt+Space", "Super+Space", reg, unreg), (Err("refused".into()), None));
        assert_eq!(*log.borrow(), ["-Alt+Space", "+Control+KeyK", "-Alt+Space", "+Super+Space", "+Alt+Space"]);
        // the old chord no longer registers either (another app took it meanwhile): that is the error to show
        let both = |_: &str| Err::<(), _>("refused".to_string());
        assert_eq!(swap("Alt+Space", "Super+Space", both, |_| {}), (Err("refused".into()), Some("refused".into())));
    }

    #[test]
    fn chords_the_recorder_writes_parse_and_broken_ones_do_not() {
        // what src/settings/chord.ts produces: Control, Alt, Shift, Super, then KeyboardEvent.code
        for ok in [super::DEFAULT, "", "Control+Alt+Shift+Super+KeyK", "Super+Digit1", "F5", "Alt+Backquote", "Super+ArrowUp", "Control+Comma"] {
            assert!(valid(ok), "{ok}");
        }
        for bad in ["Alt+", "Space+Alt", "Alt+Nope", "Alt+Space+KeyK", "+"] {
            assert!(!valid(bad), "{bad}");
        }
    }
}
