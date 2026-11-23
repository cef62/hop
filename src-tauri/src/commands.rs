//! System commands: Lock Screen, Sleep, Restart, Shut Down, Log Out, Empty Trash. Palette items (English display names; alt names add Italian, Spanish, German and French)
//! that `run` executes. The four that end the session or delete files carry a `confirm` prompt: the palette sends them
//! only after a second Enter.

use tauri::AppHandle;

use crate::apps::{Item, Kind};

/// id, name, alt names, the prompt a second Enter confirms.
type Row = (&'static str, &'static str, &'static [&'static str], Option<&'static str>);

const TABLE: [Row; 6] = [
    ("cmd:lock", "Lock Screen", &["lock", "Blocca schermo", "Bloquear pantalla", "Den Bildschirm sperren", "Verrouiller l’écran"], None),
    ("cmd:sleep", "Sleep", &["Poner en reposo", "Ruhezustand", "Suspendre l’activité"], None),
    ("cmd:restart", "Restart", &["reboot", "Riavvia", "Reiniciar", "Neustart", "Neu starten", "Redémarrer"], Some("Press Enter again to restart")),
    ("cmd:shutdown", "Shut Down", &["shutdown", "power off", "Spegni", "Apagar", "Ausschalten", "Éteindre"], Some("Press Enter again to shut down")),
    ("cmd:logout", "Log Out", &["logout", "sign out", "Esegui il logout", "Esci", "Cerrar sesión", "Abmelden", "Fermer la session"], Some("Press Enter again to log out")),
    ("cmd:empty-trash", "Empty Trash", &["trash", "Svuota il Cestino", "Vaciar papelera", "Papierkorb entleeren", "Vider la corbeille"], Some("Press Enter again to empty the Trash")),
];

/// The command items, for the index when "System commands" is on. No `path`: the palette draws their glyphs.
pub fn items() -> Vec<Item> {
    TABLE
        .iter()
        .map(|(id, name, alt, confirm)| Item {
            kind: Kind::Command,
            id: id.to_string(),
            name: name.to_string(),
            alt_names: alt.iter().map(|a| a.to_string()).collect(),
            path: None,
            confirm: confirm.map(String::from),
        })
        .collect()
}

/// An Apple Event a command sends.
#[derive(Debug, PartialEq)]
pub struct Event {
    pub class: &'static [u8; 4],
    pub id: &'static [u8; 4],
    /// the bundle id it goes to, and that app's name in errors
    pub target: &'static str,
    pub app: &'static str,
    /// what the palette says when the send fails for another reason
    pub fail: &'static str,
}

/// How a command runs.
#[derive(Debug, PartialEq)]
enum How {
    Lock,
    Sleep,
    Send(Event),
}

fn how(id: &str) -> Option<How> {
    let login = |id, fail| How::Send(Event { class: b"aevt", id, target: "com.apple.loginwindow", app: "loginwindow", fail });
    Some(match id {
        "cmd:lock" => How::Lock,
        "cmd:sleep" => How::Sleep,
        // kAERestart, kAEShutDown: loginwindow still lets apps with unsaved work cancel
        "cmd:restart" => login(b"rest", "Could not restart"),
        "cmd:shutdown" => login(b"shut", "Could not shut down"),
        // kAEReallyLogOut, the form without the macOS "Are you sure" dialog: the palette already asked
        "cmd:logout" => login(b"rlgo", "Could not log out"),
        "cmd:empty-trash" => How::Send(Event { class: b"fndr", id: b"empt", target: "com.apple.finder", app: "Finder", fail: "Could not empty the Trash" }),
        _ => return None,
    })
}

/// The palette's error for a send that failed with OSStatus `code`.
fn send_error(e: &Event, code: isize) -> String {
    match code {
        // errAEEventNotPermitted: hop is off under Automation, or the user declined the prompt
        -1743 => format!("hop is not allowed to control {} — allow it in System Settings › Privacy & Security › Automation", e.app),
        // procNotFound
        -600 => format!("{} is not running", e.app),
        _ => format!("{} (error {code})", e.fail),
    }
}

/// What `exec` does to the system, kept apart so a test can check the order without doing any of it.
pub trait Effects {
    fn hide(&mut self);
    /// Whether the lock call is there (a macOS release could drop the private symbol).
    fn can_lock(&mut self) -> bool;
    fn lock(&mut self);
    fn sleep(&mut self) -> std::io::Result<()>;
    /// Err: the OSStatus.
    fn send(&mut self, e: &Event) -> Result<(), isize>;
}

/// Runs command `id`. Lock Screen and Sleep hide the palette first (it must not be up on the lock screen or after
/// waking), but Lock checks its symbol before that, so a failure stays on screen with its error; the Apple Events hide
/// it only once sent, for the same reason.
pub fn exec(id: &str, fx: &mut impl Effects) -> Result<(), String> {
    match how(id).ok_or("Not something hop knows")? {
        How::Lock => {
            if !fx.can_lock() {
                return Err("Could not lock the screen".into());
            }
            fx.hide();
            fx.lock();
        }
        How::Sleep => {
            fx.hide();
            fx.sleep().map_err(|e| {
                eprintln!("hop: could not run pmset ({e})");
                "Could not put the Mac to sleep".to_string()
            })?;
        }
        How::Send(e) => {
            fx.send(&e).map_err(|code| {
                eprintln!("hop: Apple Event {id} to {} failed ({code})", e.target);
                send_error(&e, code)
            })?;
            fx.hide();
        }
    }
    Ok(())
}

/// The real effects, from `run`'s worker thread: the palette and the lock call go to the main thread, the Apple Event
/// is sent from here, so a first-use Automation prompt never freezes hop.
pub struct System(pub AppHandle);

impl Effects for System {
    fn hide(&mut self) {
        crate::panel::on_main(&self.0, crate::panel::hide);
    }

    fn can_lock(&mut self) -> bool {
        mac::locker().is_some()
    }

    fn lock(&mut self) {
        crate::panel::on_main(&self.0, |_| {
            #[cfg(target_os = "macos")]
            debug_assert!(objc2::MainThreadMarker::new().is_some(), "lock called off the main thread");
            mac::locker().map(|lock| lock())
        });
    }

    fn sleep(&mut self) -> std::io::Result<()> {
        let mut child = std::process::Command::new("/usr/bin/pmset").arg("sleepnow").spawn()?;
        std::thread::spawn(move || match child.wait() {
            Ok(s) if s.success() => {}
            other => eprintln!("hop: pmset sleepnow failed ({other:?})"),
        });
        Ok(())
    }

    fn send(&mut self, e: &Event) -> Result<(), isize> {
        mac::send(e.class, e.id, e.target)
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_int, c_void};

    use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventSendOptions, NSString};

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    const RTLD_LAZY: c_int = 1;

    /// `SACLockScreenImmediate` from the private login framework (what the Lock Screen menu item calls, stable since
    /// macOS 10.13), loaded at use. `None` when a macOS release drops it.
    pub fn locker() -> Option<impl FnOnce() + Send> {
        // SAFETY: NUL-terminated literals; dlopen of a system framework (served from the dyld shared cache) and dlsym
        // have no other preconditions, and a null result is checked
        let sym = unsafe {
            let h = dlopen(c"/System/Library/PrivateFrameworks/login.framework/login".as_ptr(), RTLD_LAZY);
            if h.is_null() {
                return None;
            }
            dlsym(h, c"SACLockScreenImmediate".as_ptr())
        };
        if sym.is_null() {
            return None;
        }
        // SAFETY: the symbol is `int SACLockScreenImmediate(void)`; the handle is never closed, so it stays loaded
        let lock: unsafe extern "C" fn() -> c_int = unsafe { std::mem::transmute(sym) };
        // SAFETY: as above; its result has no documented meaning, the lock itself is the outcome
        Some(move || {
            unsafe { lock() };
        })
    }

    /// The Apple Event `class`/`id` addressed to the app `target`, not sent.
    pub fn event(class: &[u8; 4], id: &[u8; 4], target: &str) -> objc2::rc::Retained<NSAppleEventDescriptor> {
        let target = NSAppleEventDescriptor::descriptorWithBundleIdentifier(&NSString::from_str(target));
        // kAutoGenerateReturnID (-1), kAnyTransactionID (0)
        NSAppleEventDescriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
            u32::from_be_bytes(*class),
            u32::from_be_bytes(*id),
            Some(&target),
            -1,
            0,
        )
    }

    /// Sends without waiting for a reply (emptying a large Trash replies only when done); a refused send, such as one
    /// the user did not allow under Automation, still fails here. Blocks while macOS shows the Automation prompt.
    /// Err: the OSStatus.
    pub fn send(class: &[u8; 4], id: &[u8; 4], target: &str) -> Result<(), isize> {
        // SAFETY: an immutable Foundation constant
        let timeout = unsafe { objc2_foundation::NSAppleEventTimeOutDefault };
        let options = NSAppleEventSendOptions::NoReply | NSAppleEventSendOptions::CanInteract;
        event(class, id, target).sendEventWithOptions_timeout_error(options, timeout).map(drop).map_err(|e| e.code())
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    pub fn locker() -> Option<fn()> {
        None
    }

    pub fn send(_: &[u8; 4], _: &[u8; 4], _: &str) -> Result<(), isize> {
        Err(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_table_is_exact() {
        let row = |i: Item| format!("{:?} {} | {} | {} | {:?} | {:?}", i.kind, i.id, i.name, i.alt_names.join(", "), i.confirm, i.path);
        assert_eq!(items().into_iter().map(row).collect::<Vec<_>>(), [
            "Command cmd:lock | Lock Screen | lock, Blocca schermo, Bloquear pantalla, Den Bildschirm sperren, Verrouiller l’écran | None | None",
            "Command cmd:sleep | Sleep | Poner en reposo, Ruhezustand, Suspendre l’activité | None | None",
            "Command cmd:restart | Restart | reboot, Riavvia, Reiniciar, Neustart, Neu starten, Redémarrer | Some(\"Press Enter again to restart\") | None",
            "Command cmd:shutdown | Shut Down | shutdown, power off, Spegni, Apagar, Ausschalten, Éteindre | Some(\"Press Enter again to shut down\") | None",
            "Command cmd:logout | Log Out | logout, sign out, Esegui il logout, Esci, Cerrar sesión, Abmelden, Fermer la session | Some(\"Press Enter again to log out\") | None",
            "Command cmd:empty-trash | Empty Trash | trash, Svuota il Cestino, Vaciar papelera, Papierkorb entleeren, Vider la corbeille | Some(\"Press Enter again to empty the Trash\") | None",
        ]);
    }

    fn event(id: &str) -> Event {
        match how(id) {
            Some(How::Send(e)) => e,
            other => panic!("{id}: {other:?}"),
        }
    }

    #[test]
    fn each_command_runs_its_own_way_and_nothing_else_runs() {
        assert_eq!(how("cmd:lock"), Some(How::Lock));
        assert_eq!(how("cmd:sleep"), Some(How::Sleep));
        let codes = |id: &str| {
            let e = event(id);
            (e.class, e.id, e.target, e.app, e.fail)
        };
        assert_eq!(codes("cmd:restart"), (b"aevt", b"rest", "com.apple.loginwindow", "loginwindow", "Could not restart"));
        assert_eq!(codes("cmd:shutdown"), (b"aevt", b"shut", "com.apple.loginwindow", "loginwindow", "Could not shut down"));
        assert_eq!(codes("cmd:logout"), (b"aevt", b"rlgo", "com.apple.loginwindow", "loginwindow", "Could not log out"));
        assert_eq!(codes("cmd:empty-trash"), (b"fndr", b"empt", "com.apple.finder", "Finder", "Could not empty the Trash"));
        for id in ["cmd:", "cmd:reboot", "lock", "/Applications/X.app", "pane:com.apple.settings.Storage", ""] {
            assert_eq!(how(id), None, "{id}");
        }
        // every item has a way to run
        assert!(items().iter().all(|i| how(&i.id).is_some()));
    }

    #[test]
    fn a_failed_send_reads_as_what_to_do() {
        let trash = event("cmd:empty-trash");
        assert_eq!(send_error(&trash, -1743), "hop is not allowed to control Finder — allow it in System Settings › Privacy & Security › Automation");
        assert_eq!(send_error(&trash, -600), "Finder is not running");
        assert_eq!(send_error(&event("cmd:restart"), -1712), "Could not restart (error -1712)");
    }

    /// Records the effects in order; nothing is hidden, locked, slept or sent.
    #[derive(Default)]
    struct Log {
        done: Vec<String>,
        no_lock: bool,
        sleep_fails: bool,
        send_fails: Option<isize>,
    }

    impl Effects for Log {
        fn hide(&mut self) {
            self.done.push("hide".into());
        }
        fn can_lock(&mut self) -> bool {
            self.done.push("can_lock".into());
            !self.no_lock
        }
        fn lock(&mut self) {
            self.done.push("lock".into());
        }
        fn sleep(&mut self) -> std::io::Result<()> {
            self.done.push("sleep".into());
            if self.sleep_fails { Err(std::io::Error::other("no pmset")) } else { Ok(()) }
        }
        fn send(&mut self, e: &Event) -> Result<(), isize> {
            self.done.push(format!("send {}", std::str::from_utf8(e.id).unwrap()));
            self.send_fails.map_or(Ok(()), Err)
        }
    }

    fn run(id: &str, fx: Log) -> (Result<(), String>, Vec<String>) {
        let mut fx = fx;
        let r = exec(id, &mut fx);
        (r, fx.done)
    }

    #[test]
    fn lock_and_sleep_hide_first_events_hide_only_once_sent() {
        assert_eq!(run("cmd:lock", Log::default()), (Ok(()), vec!["can_lock".into(), "hide".into(), "lock".into()]));
        // the symbol is gone: the palette stays up with the error
        assert_eq!(run("cmd:lock", Log { no_lock: true, ..Log::default() }), (Err("Could not lock the screen".into()), vec!["can_lock".into()]));
        assert_eq!(run("cmd:sleep", Log::default()), (Ok(()), vec!["hide".into(), "sleep".into()]));
        assert_eq!(run("cmd:sleep", Log { sleep_fails: true, ..Log::default() }).0, Err("Could not put the Mac to sleep".into()));
        for (id, code) in [("cmd:restart", "rest"), ("cmd:shutdown", "shut"), ("cmd:logout", "rlgo"), ("cmd:empty-trash", "empt")] {
            assert_eq!(run(id, Log::default()), (Ok(()), vec![format!("send {code}"), "hide".into()]), "{id}");
            let (r, done) = run(id, Log { send_fails: Some(-1743), ..Log::default() });
            assert!(r.unwrap_err().starts_with("hop is not allowed to control"), "{id}");
            assert_eq!(done, [format!("send {code}")], "{id}: a failed send hid the palette");
        }
        assert_eq!(run("cmd:nope", Log::default()), (Err("Not something hop knows".into()), vec![]));
    }

    /// Builds the descriptors only: nothing here is ever sent.
    #[cfg(target_os = "macos")]
    #[test]
    fn apple_events_are_addressed_by_their_four_char_codes() {
        for id in ["cmd:restart", "cmd:shutdown", "cmd:logout", "cmd:empty-trash"] {
            let e = event(id);
            let d = mac::event(e.class, e.id, e.target);
            assert_eq!((d.eventClass().to_be_bytes(), d.eventID().to_be_bytes()), (*e.class, *e.id), "{id}");
        }
        assert_eq!(mac::event(b"aevt", b"rest", "com.apple.loginwindow").eventClass(), 0x61657674); // 'aevt'
    }
}
