use tauri::Manager;

pub mod apps;
mod history;
mod hotkey;
mod icons;
mod launch;
mod login;
mod panel;
mod settings;

/// What each window may invoke: the palette (`main`) and Settings, nothing in common.
const MAIN_COMMANDS: &[&str] = &["hide_palette", "card_frame", "perf_mark", "list_apps", "frecency", "launch"];
const SETTINGS_COMMANDS: &[&str] = &["get_settings", "set_settings", "pick_path", "set_login"];

/// Custom commands are open to every window (no app ACL manifest in build.rs, so capabilities do not restrict them):
/// this guard is the boundary. A command missing from both lists is denied everywhere.
fn allowed(label: &str, command: &str) -> bool {
    match label {
        "main" => MAIN_COMMANDS.contains(&command),
        settings::LABEL => SETTINGS_COMMANDS.contains(&command),
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
    let item = |id: &str, text: &str, on: bool| MenuItem::with_id(app, id, text, on, None::<&str>);
    let menu = Menu::with_items(app, &[
        &item("open", "Open hop", true)?,
        &item("settings", "Settings…", true)?,
        // shortcut: disabled until the updater lands (M5)
        &item("updates", "Check for Updates…", false)?,
        &PredefinedMenuItem::separator(app)?,
        &item("quit", "Quit hop", true)?,
    ])?;
    tauri::tray::TrayIconBuilder::with_id("main")
        .tooltip("hop")
        // a black glyph the menu bar tints for light and dark (icons/tray-template.svg is the source)
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))?)
        .icon_as_template(true)
        .menu(&menu)
        .on_menu_event(|app, ev| match ev.id().as_ref() {
            "open" => panel::show(app),
            "settings" => settings::open(app),
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
        .register_asynchronous_uri_scheme_protocol("icon", icons::handle)
        .setup(move |app| {
            let h = app.handle();
            // first: a login item launched beside a running hop (or a dev build beside the installed one, which share
            // the identifier) leaves before it registers anything
            if let Ok(dir) = h.path().app_data_dir()
                && !login::hold_instance_lock(&dir)
            {
                eprintln!("hop: another hop is running (quit it first when running a dev build); this one exits");
                std::process::exit(0);
            }
            let first_run = settings::setup(h);
            panel::setup(h);
            hotkey::start(h, &settings::current(h).hotkey, started);
            history::setup(h);
            apps::setup(h); // scans on its own thread, after the hotkey is live
            if let Err(e) = tray(h) {
                eprintln!("hop: no menu-bar icon ({e})");
            }
            if login::startup(h, first_run) && first_run {
                settings::persist(h); // records the first run as done
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
            login::set_login
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
        for c in MAIN_COMMANDS {
            assert!(allowed("main", c) && !allowed("settings", c), "{c}");
        }
        for c in SETTINGS_COMMANDS {
            assert!(allowed("settings", c) && !allowed("main", c), "{c}");
        }
        assert!(!allowed("", "launch") && !allowed("whatsnew", "get_settings") && !allowed("main", "unknown"));
    }
}
