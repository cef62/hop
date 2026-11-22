//! The palette window: placement, show/hide, and the macOS window setup.

use std::sync::Mutex;
use std::time::Instant;

use tauri::{AppHandle, Emitter, LogicalPosition, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

/// Logical size of the fixed palette window (tauri.conf.json). Never resized on show.
const WINDOW: (f64, f64) = (640.0, 460.0);

/// Which step the window must take; `Visibility` decides it, `apply` performs it.
#[derive(Debug, PartialEq)]
pub enum Step {
    Show,
    Hide,
    Stay,
}

/// The palette's own idea of whether it is up. Kept here instead of asking the window: `is_visible`/`is_focused` lag
/// behind a show that is still queued, so a rapid second hotkey press read them as "hidden" and showed again.
#[derive(Default)]
pub struct Visibility {
    shown: bool,
    /// The window got focus since the last show. A blur before that is stale (it belongs to the previous
    /// show/hide, or macOS sent it while activating hop) and must not hide the palette that is just coming up.
    focused: bool,
}

impl Visibility {
    pub fn toggle(&mut self) -> Step {
        if self.shown { self.hide() } else { self.show() }
    }

    pub fn show(&mut self) -> Step {
        if self.shown {
            return Step::Stay;
        }
        *self = Visibility { shown: true, focused: false };
        Step::Show
    }

    pub fn hide(&mut self) -> Step {
        if !self.shown {
            return Step::Stay;
        }
        self.shown = false;
        Step::Hide
    }

    pub fn focus(&mut self, focused: bool) -> Step {
        match (self.shown, focused, self.focused) {
            (false, _, _) => Step::Stay,
            (true, true, _) => {
                self.focused = true;
                Step::Stay
            }
            (true, false, true) => self.hide(),
            (true, false, false) => Step::Stay,
        }
    }
}

/// Top-left of the palette window in logical points: on the monitor under the cursor (else the first), centered
/// horizontally, top at a third of the monitor height, kept on screen.
///
/// tao on macOS mixes scales: the cursor is points times the *primary* monitor's scale, each monitor's position and
/// size are points times *its own* scale. Everything is brought back to points before comparing, and the result is a
/// LogicalPosition, which tao passes to Cocoa unchanged (a PhysicalPosition would be divided by the window's current
/// scale, the wrong one when the window is still on another monitor).
pub fn place(
    cursor: PhysicalPosition<f64>,
    primary_scale: f64,
    monitors: &[(PhysicalPosition<i32>, PhysicalSize<u32>, f64)],
    window: (f64, f64),
) -> Option<LogicalPosition<f64>> {
    let (cx, cy) = (cursor.x / primary_scale, cursor.y / primary_scale);
    let screens: Vec<_> = monitors
        .iter()
        .map(|(p, s, k)| (p.x as f64 / k, p.y as f64 / k, s.width as f64 / k, s.height as f64 / k))
        .collect();
    let &(x, y, w, h) = screens
        .iter()
        .find(|(x, y, w, h)| cx >= *x && cx < x + w && cy >= *y && cy < y + h)
        .or(screens.first())?;
    Some(LogicalPosition::new(x + (w - window.0) / 2.0, y + (h / 3.0).min(h - window.1).max(0.0)))
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

/// What the frontend gets with `palette-shown` and `palette-hidden`: the style to apply before the next paint (so a
/// changed setting lands while the palette is hidden), whether to report timings (`HOP_PERF=1`), and the update waiting
/// to install, for the footer pill. Read at each show: an update found while the palette is up appears at the next
/// show, so nothing moves under the user's typing.
#[derive(serde::Serialize, Clone)]
struct Look {
    style: &'static str,
    perf: bool,
    update: Option<String>,
}

fn look(app: &AppHandle) -> Look {
    Look { style: crate::settings::palette_style(app), perf: perf_enabled(), update: crate::update::pending(app) }
}

fn perf_enabled() -> bool {
    std::env::var_os("HOP_PERF").is_some()
}

/// How a hide hands the focus back.
#[derive(Debug, PartialEq)]
pub enum GiveBack {
    /// Hide the whole app: macOS refocuses the app that was active before (velo's trick).
    HideApp,
    /// Activate this pid: the app the palette came up over. Used while Settings or What's new is open, which `HideApp` would hide.
    Activate(i32),
    /// Leave the focus where it is: the palette came up over hop's own Settings window, or it lost the focus (a click
    /// on Settings or another app already moved it, and activating the old app would steal it back).
    Stay,
}

/// `window_open`: Settings or What's new exists. `blur`: the hide comes from the palette losing the focus, not from Esc,
/// the hotkey or a launch.
pub fn give_back(window_open: bool, previous: Option<i32>, blur: bool) -> GiveBack {
    match (window_open, previous, blur) {
        (false, _, _) => GiveBack::HideApp,
        (true, _, true) | (true, None, _) => GiveBack::Stay,
        (true, Some(pid), false) => GiveBack::Activate(pid),
    }
}

/// The frontmost app when the palette last showed, unless it was hop itself.
static PREVIOUS: Mutex<Option<i32>> = Mutex::new(None);

/// When the last show started, for the hotkey → typeable measurement.
static SHOW_STARTED: Mutex<Option<Instant>> = Mutex::new(None);

/// Runs a `Visibility` transition and performs the step it returns. `blur`: see `give_back`.
fn apply(app: &AppHandle, blur: bool, step: impl FnOnce(&mut Visibility) -> Step) {
    let step = step(&mut app.state::<Mutex<Visibility>>().lock().unwrap());
    let Some(w) = main_window(app) else { return };
    match step {
        Step::Show => {
            *SHOW_STARTED.lock().unwrap() = Some(Instant::now());
            #[cfg(target_os = "macos")]
            {
                *PREVIOUS.lock().unwrap() = mac::frontmost_other();
            }
            // positioned before it is shown, so it never flashes on the monitor it was last on
            let pos = app.cursor_position().ok().and_then(|c| {
                let primary = app.primary_monitor().ok().flatten()?;
                // the active display list is empty while the display sleeps (seen with the screen locked)
                let ms = app.available_monitors().ok().filter(|ms| !ms.is_empty()).unwrap_or_else(|| vec![primary.clone()]);
                let ms: Vec<_> = ms.iter().map(|m| (*m.position(), *m.size(), m.scale_factor())).collect();
                place(c, primary.scale_factor(), &ms, WINDOW)
            });
            if let Some(pos) = pos {
                w.set_position(pos).ok();
            }
            #[cfg(target_os = "macos")]
            app.show().ok(); // undo the app.hide() of the last hide, or the window stays ordered out
            w.show().ok();
            w.set_focus().ok();
            w.emit("palette-shown", look(app)).ok();
        }
        Step::Hide => {
            w.hide().ok();
            #[cfg(target_os = "macos")]
            match give_back(crate::settings::is_open(app) || crate::whatsnew::is_open(app), *PREVIOUS.lock().unwrap(), blur) {
                GiveBack::HideApp => drop(app.hide()),
                GiveBack::Activate(pid) => mac::activate(pid),
                GiveBack::Stay => {}
            }
            w.emit("palette-hidden", look(app)).ok();
        }
        Step::Stay => {}
    }
}

/// Whether the palette is up: an update's relaunch waits for it to hide.
pub fn is_shown(app: &AppHandle) -> bool {
    app.state::<Mutex<Visibility>>().lock().unwrap_or_else(|p| p.into_inner()).shown
}

/// The hotkey: hides the palette when it is up, else shows it.
pub fn toggle(app: &AppHandle) {
    apply(app, false, Visibility::toggle);
}

/// Relaunching hop (`RunEvent::Reopen`) or the menu-bar icon's Open: the ways in when the hotkey is not registered.
pub fn show(app: &AppHandle) {
    apply(app, false, Visibility::show);
}

/// `WindowEvent::Focused` of the palette window: clicking away hides it.
pub fn focus_changed(app: &AppHandle, focused: bool) {
    apply(app, true, |v| v.focus(focused));
}

/// Esc in the palette, or a launch.
pub fn hide(app: &AppHandle) {
    apply(app, false, Visibility::hide);
}

/// Hides the palette without handing the focus to another app: Settings or What's new is about to take it.
pub fn hide_for_window(app: &AppHandle) {
    apply(app, true, Visibility::hide);
}

#[tauri::command]
pub fn hide_palette(app: AppHandle) {
    hide(&app);
}

/// Timings from the frontend, printed to stderr only with `HOP_PERF=1`: `shown` when the input is focused and painted
/// after a show (measured here, from the start of the show), `key` with the keystroke → painted time in ms.
#[tauri::command]
pub fn perf_mark(kind: String, ms: Option<f64>) {
    if !perf_enabled() {
        return;
    }
    match (kind.as_str(), ms) {
        ("shown", _) => {
            if let Some(t) = SHOW_STARTED.lock().unwrap().take() {
                eprintln!("hop perf: shown {:.1}", t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        ("key", Some(ms)) if ms.is_finite() => eprintln!("hop perf: key {ms:.1}"),
        _ => {}
    }
}

/// The card's rect in the window (CSS px = points, top-left origin) and corner radius: the vibrancy view is fitted to
/// it, since the fixed-size window is larger than the card and a full-window blur would show around it.
#[tauri::command]
pub fn card_frame(window: WebviewWindow, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    #[cfg(target_os = "macos")]
    if [x, y, width, height, radius].iter().all(|v| v.is_finite()) {
        let w = window.clone();
        window.run_on_main_thread(move || mac::fit_effect_view(&w, (x, y, width, height), radius)).ok();
    }
}

/// One-time setup of the palette window, on the main thread (tauri's `setup`).
pub fn setup(app: &AppHandle) {
    app.manage(Mutex::new(Visibility::default()));
    #[cfg(target_os = "macos")]
    if let Some(w) = main_window(app) {
        mac::float_over_everything(&w);
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2_app_kit::{
        NSApplicationActivationOptions, NSAutoresizingMaskOptions, NSRunningApplication, NSStatusWindowLevel, NSView, NSWindow,
        NSWindowCollectionBehavior, NSWorkspace,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use tauri::WebviewWindow;

    /// The tag window-vibrancy (behind tauri's `windowEffects`) puts on its NSVisualEffectView.
    const BLUR_VIEW_TAG: isize = 91376254;

    fn ns_window(w: &WebviewWindow) -> Option<&NSWindow> {
        // SAFETY: tauri hands out the live NSWindow of this window; callers are on the main thread
        w.ns_window().ok().and_then(|p| unsafe { (p as *const NSWindow).as_ref() })
    }

    /// Every Space, including a full-screen app's own Space, without switching to it; not in Mission Control
    /// (Transient) or the window cycle (IgnoresCycle). Status level puts it above a full-screen app's windows, which
    /// tauri's alwaysOnTop (floating level) does not.
    pub fn float_over_everything(w: &WebviewWindow) {
        let Some(ns) = ns_window(w) else { return };
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns.setLevel(NSStatusWindowLevel);
    }

    /// The frontmost app's pid, unless it is hop.
    pub fn frontmost_other() -> Option<i32> {
        let pid = NSWorkspace::sharedWorkspace().frontmostApplication()?.processIdentifier();
        (pid != std::process::id() as i32).then_some(pid)
    }

    /// hop is active (the palette had the focus), so macOS lets it hand activation over.
    pub fn activate(pid: i32) {
        if let Some(a) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            a.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }

    pub fn fit_effect_view(w: &WebviewWindow, (x, y, width, height): (f64, f64, f64, f64), radius: f64) {
        let Some(content) = ns_window(w).and_then(|ns| ns.contentView()) else { return };
        let Some(blur) = content.viewWithTag(BLUR_VIEW_TAG) else { return };
        let bounds = content.bounds();
        // AppKit's y axis points up from the bottom
        let frame = NSRect::new(NSPoint::new(x, bounds.size.height - y - height), NSSize::new(width, height));
        blur.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
        NSView::setFrame(&blur, frame);
        // the styles differ in radius (12 / 16 px); window-vibrancy applied the config's radius through this same
        // NSVisualEffectView method, which AppKit implements but does not document
        // SAFETY: the view is window-vibrancy's NSVisualEffectView subclass, which responds to setCornerRadius:
        let () = unsafe { objc2::msg_send![&*blur, setCornerRadius: radius] };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(x: i32, y: i32, w: u32, h: u32, scale: f64) -> (PhysicalPosition<i32>, PhysicalSize<u32>, f64) {
        (PhysicalPosition::new(x, y), PhysicalSize::new(w, h), scale)
    }

    #[test]
    fn a_hide_gives_focus_back_without_hiding_an_open_settings_window() {
        assert_eq!(give_back(false, Some(42), false), GiveBack::HideApp);
        assert_eq!(give_back(false, None, false), GiveBack::HideApp);
        // Settings is open (maybe behind other windows, minimized or on another Space): hiding the app would hide it,
        // staying active would bring it forward; the app the palette came up over gets the focus instead
        assert_eq!(give_back(true, Some(42), false), GiveBack::Activate(42));
        // the palette came up over hop's own Settings window: it keeps the focus
        assert_eq!(give_back(true, None, false), GiveBack::Stay);
        // a click on Settings (or another app) blurred the palette: the focus already moved, nothing is activated
        assert_eq!(give_back(true, Some(42), true), GiveBack::Stay);
    }

    #[test]
    fn toggle_shows_then_hides() {
        let mut v = Visibility::default();
        assert_eq!(v.toggle(), Step::Show);
        assert_eq!(v.toggle(), Step::Hide);
        assert_eq!(v.toggle(), Step::Show);
    }

    #[test]
    fn rapid_double_press_hides_even_before_focus_arrives() {
        let mut v = Visibility::default();
        assert_eq!(v.toggle(), Step::Show);
        assert_eq!(v.toggle(), Step::Hide); // no Focused(true) in between
        assert_eq!(v.focus(true), Step::Stay); // late focus of the first show changes nothing
        assert_eq!(v.focus(false), Step::Stay);
        assert_eq!(v.toggle(), Step::Show);
    }

    #[test]
    fn blur_during_show_is_ignored_blur_after_focus_hides() {
        let mut v = Visibility::default();
        assert_eq!(v.show(), Step::Show);
        assert_eq!(v.focus(false), Step::Stay); // stale blur while activating
        assert_eq!(v.focus(true), Step::Stay);
        assert_eq!(v.focus(false), Step::Hide);
        assert_eq!(v.focus(false), Step::Stay); // the blur our own hide causes
    }

    #[test]
    fn a_new_show_needs_a_new_focus_before_blur_hides() {
        let mut v = Visibility::default();
        v.show();
        v.focus(true);
        assert_eq!(v.hide(), Step::Hide);
        assert_eq!(v.show(), Step::Show);
        assert_eq!(v.focus(false), Step::Stay);
    }

    #[test]
    fn show_and_hide_are_idempotent() {
        let mut v = Visibility::default();
        assert_eq!(v.hide(), Step::Stay);
        assert_eq!(v.show(), Step::Show);
        assert_eq!(v.show(), Step::Stay); // Reopen while up
        assert_eq!(v.hide(), Step::Hide); // Esc
        assert_eq!(v.hide(), Step::Stay); // Esc again, already queued
    }

    #[test]
    fn places_on_single_retina_monitor() {
        // 1512x982 points at 2x
        let p = place(PhysicalPosition::new(400.0, 300.0), 2.0, &[mon(0, 0, 3024, 1964, 2.0)], WINDOW).unwrap();
        assert_eq!((p.x, p.y), ((1512.0 - 640.0) / 2.0, 982.0 / 3.0));
    }

    #[test]
    fn picks_second_monitor_left_of_primary_at_other_scale() {
        // primary 1512x982 pt @2x at (0,0); external 1920x1080 pt @1x at (-1920,-200)
        let ms = [mon(0, 0, 3024, 1964, 2.0), mon(-1920, -200, 1920, 1080, 1.0)];
        // cursor at (-1000, 100) points, reported in primary-scale "physical" units
        let p = place(PhysicalPosition::new(-2000.0, 200.0), 2.0, &ms, WINDOW).unwrap();
        assert_eq!((p.x, p.y), (-1920.0 + (1920.0 - 640.0) / 2.0, -200.0 + 360.0));
    }

    #[test]
    fn picks_second_monitor_at_higher_scale_than_primary() {
        // primary 1920x1080 pt @1x; Retina 1512x982 pt @2x to the right at (1920, 0)
        let ms = [mon(0, 0, 1920, 1080, 1.0), mon(3840, 0, 3024, 1964, 2.0)];
        let p = place(PhysicalPosition::new(2500.0, 50.0), 1.0, &ms, WINDOW).unwrap();
        assert_eq!(p.x, 1920.0 + (1512.0 - 640.0) / 2.0);
    }

    #[test]
    fn stays_on_a_short_monitor() {
        // 1024x600 points: a third down (200) + 460 would overflow; pulled up to 140
        let p = place(PhysicalPosition::new(10.0, 10.0), 1.0, &[mon(0, 0, 1024, 600, 1.0)], WINDOW).unwrap();
        assert_eq!(p.y, 140.0);
    }

    #[test]
    fn cursor_off_every_monitor_falls_back_to_first() {
        let p = place(PhysicalPosition::new(-5000.0, -5000.0), 1.0, &[mon(0, 0, 1920, 1080, 1.0)], WINDOW).unwrap();
        assert_eq!(p.x, 640.0);
        assert!(place(PhysicalPosition::new(0.0, 0.0), 1.0, &[], WINDOW).is_none());
    }
}
