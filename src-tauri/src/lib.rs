use tauri::Manager;

pub mod apps;
mod history;
mod hotkey;
mod icons;
mod launch;
mod login;
mod panel;
mod settings;
mod update;
mod whatsnew;

/// What each window may invoke: the palette (`main`), Settings and What's new. Only `open_whatsnew` is shared (the
/// palette's pill and Settings' "What's new…").
const MAIN_COMMANDS: &[&str] = &["hide_palette", "card_frame", "perf_mark", "list_apps", "frecency", "launch", "open_settings", "open_whatsnew"];
const SETTINGS_COMMANDS: &[&str] = &["get_settings", "set_settings", "pick_path", "set_login", "close_settings", "check_update", "open_whatsnew"];
const WHATSNEW_COMMANDS: &[&str] = &["whatsnew_info", "whatsnew_install", "whatsnew_close"];

/// Custom commands are open to every window (no app ACL manifest in build.rs, so capabilities do not restrict them):
/// this guard is the boundary. A command missing from both lists is denied everywhere.
fn allowed(label: &str, command: &str) -> bool {
    match label {
        "main" => MAIN_COMMANDS.contains(&command),
        settings::LABEL => SETTINGS_COMMANDS.contains(&command),
        whatsnew::LABEL => WHATSNEW_COMMANDS.contains(&command),
        _ => false,
    }
}

fn with_window_guard<R: tauri::Runtime>(handler: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        if !allowed(invoke.message.webview_ref().label(), invoke.message.command()) {
            invoke.resolver.reject("not allowed from this window");
            return true;
        }
        handler(invoke)
    }
}

/// Quit from the menu-bar icon. The launch history is written first: a launch in the last two seconds is still only
/// in memory (`history::DEBOUNCE`).
fn quit(app: &tauri::AppHandle) {
    history::flush_now(app);
    app.exit(0);
}

/// The menu-bar icon, always shown (plan §4.9): the way to Settings and Quit when the hotkey is taken.
fn tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let item = |id: &str, text: &str| MenuItem::with_id(app, id, text, true, None::<&str>);
    // its text names a found update (update.rs)
    let updates = item("updates", &update::tray_text(None))?;
    let menu = Menu::with_items(app, &[
        &item("open", "Open hop")?,
        &item("settings", "Settings…")?,
        &updates,
        &PredefinedMenuItem::separator(app)?,
        &item("quit", "Quit hop")?,
    ])?;
    app.manage(update::TrayItem(updates));
    tauri::tray::TrayIconBuilder::with_id("main")
        .tooltip("hop")
        // a black glyph the menu bar tints for light and dark (icons/tray-template.svg is the source)
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => panel::show(app),
            "settings" => settings::open(app),
            "updates" => update::tray_clicked(app),
            "quit" => quit(app),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let started = std::time::Instant::now();
    let mut app = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // the LaunchAgent; only it passes --hidden
        .plugin(tauri_plugin_autostart::Builder::new().app_name(login::AUTOSTART_NAME).arg(login::HIDDEN_ARG).build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .register_asynchronous_uri_scheme_protocol("icon", icons::handle)
        .setup(move |app| {
            let h = app.handle();
            // first: a login item launched beside a running hop (or a dev build beside the installed one, which share
            // the identifier) leaves before it registers anything. Only the relaunch after an update, whose old process
            // is still exiting, waits for the lock; the mark is read here and consumed once the lock is ours
            if let Ok(dir) = h.path().app_data_dir() {
                if !login::hold_instance_lock(&dir, login::lock_wait(login::restart_mark_fresh(&dir))) {
                    eprintln!("hop: another hop is running (quit it first when running a dev build); this one exits");
                    std::process::exit(0);
                }
                login::take_restart_mark(&dir); // hop starts hidden either way
            }
            let first_run = settings::setup(h);
            panel::setup(h);
            hotkey::start(h, &settings::current(h).hotkey, started);
            history::setup(h);
            apps::setup(h); // scans on its own thread, after the hotkey is live
            update::setup(h); // before the tray, which reads the pending version
            if let Err(e) = tray(h) {
                eprintln!("hop: no menu-bar icon ({e})");
            }
            if login::startup(h, first_run) && first_run {
                settings::mark_login_applied(h);
            }
            Ok(())
        })
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::Focused(f) = ev
                && w.label() == "main"
            {
                panel::focus_changed(w.app_handle(), *f);
            }
        })
        .invoke_handler(with_window_guard(tauri::generate_handler![
            panel::hide_palette,
            panel::card_frame,
            panel::perf_mark,
            apps::list_apps,
            history::frecency,
            launch::launch,
            settings::get_settings,
            settings::set_settings,
            settings::pick_path,
            settings::open_settings,
            settings::close_settings,
            login::set_login,
            update::check_update,
            whatsnew::open_whatsnew,
            whatsnew::whatsnew_info,
            whatsnew::whatsnew_install,
            whatsnew::whatsnew_close
        ]))
        .build(tauri::generate_context!())
        .expect("error while running hop");
    // no Dock icon, not in ⌘Tab. Set before run: tao applies its policy (Regular by default) in
    // applicationDidFinishLaunching, which overrides Info.plist's LSUIElement; from setup it lands ~60 ms too late
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    app.run(|_app, _ev| match _ev {
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => panel::show(_app),
        tauri::RunEvent::Exit => history::flush_now(_app),
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_window_calls_only_its_own_commands() {
        let lists = [("main", MAIN_COMMANDS), ("settings", SETTINGS_COMMANDS), ("whatsnew", WHATSNEW_COMMANDS)];
        for (label, list) in lists {
            for c in list {
                assert!(allowed(label, c), "{label} {c}");
                // the only shared command is opening What's new
                for (other, _) in lists.iter().filter(|(o, _)| *o != label) {
                    assert_eq!(allowed(other, c), *c == "open_whatsnew" && *other != "whatsnew", "{other} {c}");
                }
            }
        }
        assert!(!allowed("", "launch") && !allowed("whatsnew", "get_settings") && !allowed("main", "unknown"));
        assert!(!allowed("main", "check_update") && !allowed("main", "whatsnew_install") && !allowed("settings", "whatsnew_install"));
    }
}
