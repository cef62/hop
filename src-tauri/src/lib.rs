use tauri::Manager;

mod hotkey;
mod panel;

pub fn run() {
    let started = std::time::Instant::now();
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            // no Dock icon, not in ⌘Tab; the palette window is the whole UI
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            panel::setup(app.handle());
            hotkey::register(app.handle(), started);
            Ok(())
        })
        // one window today; M3/M5 add settings and whatsnew: then guard these per window label as velo does
        .on_window_event(|w, ev| {
            if let tauri::WindowEvent::Focused(f) = ev {
                panel::focus_changed(w.app_handle(), *f);
            }
        })
        .invoke_handler(tauri::generate_handler![panel::hide_palette, panel::card_frame])
        .build(tauri::generate_context!())
        .expect("error while running hop")
        .run(|_app, _ev| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _ev {
                panel::show(_app);
            }
        });
}
