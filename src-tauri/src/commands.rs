//! System commands: Lock Screen, Sleep, Restart, Shut Down, Log Out, Empty Trash. Palette items (English names only)
//! that `run` executes. The four that end the session or delete files carry a `confirm` prompt: the palette sends them
//! only after a second Enter.

use tauri::AppHandle;

use crate::apps::{Item, Kind};

/// id, name, alt names, the prompt a second Enter confirms.
type Row = (&'static str, &'static str, &'static [&'static str], Option<&'static str>);

const TABLE: [Row; 6] = [
    ("cmd:lock", "Lock Screen", &["lock"], None),
    ("cmd:sleep", "Sleep", &[], None),
    ("cmd:restart", "Restart", &["reboot"], Some("Press Enter again to restart")),
    ("cmd:shutdown", "Shut Down", &["shutdown", "power off"], Some("Press Enter again to shut down")),
    ("cmd:logout", "Log Out", &["logout", "sign out"], Some("Press Enter again to log out")),
    ("cmd:empty-trash", "Empty Trash", &["trash"], Some("Press Enter again to empty the Trash")),
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

const LOGINWINDOW: &str = "com.apple.loginwindow";

/// How a command runs.
#[derive(Debug, PartialEq)]
enum How {
    Lock,
    Sleep,
    /// An Apple Event: class, id, the target's bundle id, and what the palette says when it cannot be sent.
    Event(&'static [u8; 4], &'static [u8; 4], &'static str, &'static str),
}

fn how(id: &str) -> Option<How> {
    Some(match id {
        "cmd:lock" => How::Lock,
        "cmd:sleep" => How::Sleep,
        // kAERestart, kAEShutDown: loginwindow still lets apps with unsaved work cancel
        "cmd:restart" => How::Event(b"aevt", b"rest", LOGINWINDOW, "Could not restart"),
        "cmd:shutdown" => How::Event(b"aevt", b"shut", LOGINWINDOW, "Could not shut down"),
        // kAEReallyLogOut, the form without the macOS "Are you sure" dialog: the palette already asked
        "cmd:logout" => How::Event(b"aevt", b"rlgo", LOGINWINDOW, "Could not log out"),
        "cmd:empty-trash" => How::Event(b"fndr", b"empt", "com.apple.finder", "Could not empty the Trash"),
        _ => return None,
    })
}

/// errAEEventNotPermitted: the user turned hop off in Privacy & Security › Automation, or declined the prompt.
const NOT_PERMITTED: isize = -1743;

/// The palette's error for a send that failed with `code`.
fn send_error(fail: &str, code: isize) -> String {
    if code == NOT_PERMITTED {
        format!("{fail}: allow hop in System Settings › Privacy & Security › Automation")
    } else {
        format!("{fail} (error {code})")
    }
}

/// Runs command `id`. Lock Screen and Sleep hide the palette first (it must not be up on the lock screen or after
/// waking); the Apple Events hide it only once sent, so a failure stays on screen with its error.
pub fn exec(app: &AppHandle, id: &str) -> Result<(), String> {
    match how(id).ok_or("Not something hop knows")? {
        How::Lock => {
            let lock = mac::locker().ok_or("Could not lock the screen")?;
            crate::panel::hide(app);
            lock();
        }
        How::Sleep => {
            crate::panel::hide(app);
            let mut child = std::process::Command::new("/usr/bin/pmset").arg("sleepnow").spawn().map_err(|e| {
                eprintln!("hop: could not run pmset ({e})");
                "Could not put the Mac to sleep".to_string()
            })?;
            std::thread::spawn(move || match child.wait() {
                Ok(s) if s.success() => {}
                other => eprintln!("hop: pmset sleepnow failed ({other:?})"),
            });
        }
        How::Event(class, event, target, fail) => {
            mac::send(class, event, target).map_err(|code| {
                eprintln!("hop: Apple Event {id} to {target} failed ({code})");
                send_error(fail, code)
            })?;
            crate::panel::hide(app);
        }
    }
    Ok(())
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
    pub fn locker() -> Option<impl FnOnce()> {
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
    /// the user did not allow under Automation, still fails here. Err: the OSStatus.
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
            "Command cmd:lock | Lock Screen | lock | None | None",
            "Command cmd:sleep | Sleep |  | None | None",
            "Command cmd:restart | Restart | reboot | Some(\"Press Enter again to restart\") | None",
            "Command cmd:shutdown | Shut Down | shutdown, power off | Some(\"Press Enter again to shut down\") | None",
            "Command cmd:logout | Log Out | logout, sign out | Some(\"Press Enter again to log out\") | None",
            "Command cmd:empty-trash | Empty Trash | trash | Some(\"Press Enter again to empty the Trash\") | None",
        ]);
    }

    #[test]
    fn each_command_runs_its_own_way_and_nothing_else_runs() {
        assert_eq!(how("cmd:lock"), Some(How::Lock));
        assert_eq!(how("cmd:sleep"), Some(How::Sleep));
        assert_eq!(how("cmd:restart"), Some(How::Event(b"aevt", b"rest", "com.apple.loginwindow", "Could not restart")));
        assert_eq!(how("cmd:shutdown"), Some(How::Event(b"aevt", b"shut", "com.apple.loginwindow", "Could not shut down")));
        assert_eq!(how("cmd:logout"), Some(How::Event(b"aevt", b"rlgo", "com.apple.loginwindow", "Could not log out")));
        assert_eq!(how("cmd:empty-trash"), Some(How::Event(b"fndr", b"empt", "com.apple.finder", "Could not empty the Trash")));
        for id in ["cmd:", "cmd:reboot", "lock", "/Applications/X.app", "pane:com.apple.settings.Storage", ""] {
            assert_eq!(how(id), None, "{id}");
        }
        // every item has a way to run
        assert!(items().iter().all(|i| how(&i.id).is_some()));
    }

    #[test]
    fn a_refused_send_says_where_to_allow_it() {
        assert_eq!(send_error("Could not empty the Trash", -1743), "Could not empty the Trash: allow hop in System Settings › Privacy & Security › Automation");
        assert_eq!(send_error("Could not restart", -600), "Could not restart (error -600)");
    }

    /// Builds the descriptors only: nothing here is ever sent.
    #[cfg(target_os = "macos")]
    #[test]
    fn apple_events_are_addressed_by_their_four_char_codes() {
        for id in ["cmd:restart", "cmd:shutdown", "cmd:logout", "cmd:empty-trash"] {
            let Some(How::Event(class, event, target, _)) = how(id) else { panic!("{id}") };
            let e = mac::event(class, event, target);
            assert_eq!((e.eventClass().to_be_bytes(), e.eventID().to_be_bytes()), (*class, *event), "{id}");
        }
        assert_eq!(mac::event(b"aevt", b"rest", "com.apple.loginwindow").eventClass(), 0x61657674); // 'aevt'
    }
}
