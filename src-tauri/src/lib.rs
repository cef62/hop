use tauri::Manager;

pub mod apps;
mod history;
mod hotkey;
mod icons;
mod launch;
mod panel;
mod settings;

/// Custom commands are open to every window (no app ACL manifest in build.rs, so capabilities do not restrict them):
/// this guard is the boundary. Only the palette window (`main`) may call them; M3/M5 windows get their own list.
fn allowed(label: &str) -> bool {
    label == "main"
}

fn with_window_guard<R: tauri::Runtime>(handler: impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    move |invoke| {
        if !allowed(invoke.message.webview_ref().label()) {
            invoke.resolver.reject("not allowed from this window");
            return true;
        }
        handler(invoke)
    }
}

pub fn run() {
    let started = std::time::Instant::now();
    let mut app = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .register_asynchronous_uri_scheme_protocol("icon", icons::handle)
        .setup(move |app| {
            panel::setup(app.handle());
            hotkey::register(app.handle(), started);
            history::setup(app.handle());
            apps::setup(app.handle()); // scans on its own thread, after the hotkey is live
            Ok(())
        })
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::Focused(f) = ev {
                panel::focus_changed(w.app_handle(), *f);
            }
        })
        .invoke_handler(with_window_guard(tauri::generate_handler![
            panel::hide_palette,
            panel::card_frame,
            panel::perf_mark,
            apps::list_apps,
            history::frecency,
            launch::launch
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
    #[test]
    fn only_the_palette_window_calls_commands() {
        assert!(super::allowed("main"));
        assert!(!super::allowed("settings"));
        assert!(!super::allowed(""));
    }
}
