// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

mod background;
mod cleanup;
mod commands;
mod diag;
mod settings;

/// Passed to the elevated copy started by "Run as admin".
pub const ELEVATED_ARG: &str = "--elevated";

#[cfg(windows)]
extern "system" {
    fn FreeConsole() -> i32;
}

fn main() {
    let background = std::env::args().any(|a| a == background::BACKGROUND_ARG);
    if std::env::args().any(|a| a == ELEVATED_ARG) {
        // Debug builds are console programs, so the elevated relaunch would get
        // its own cmd window: drop it (release builds have none).
        #[cfg(windows)]
        unsafe {
            FreeConsole();
        }
        // Let the copy that launched us quit first, so single-instance
        // doesn't hand this launch back to it.
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }
    tauri::Builder::default()
        // a second launch (Start menu while Sift sits in the tray) opens the window
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| background::show_window(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![background::BACKGROUND_ARG]),
        ))
        .manage(commands::AppState::default())
        .setup(move |app| {
            diag::init(app);
            let s = settings::load(app);
            let days = s.auto_purge_days;
            *app.state::<commands::AppState>().settings.lock().unwrap() = s;
            cleanup::init(app, days)?;
            settings::watch_palette(app.handle().clone());
            background::init(app);
            commands::restore_last(app.handle());
            // the window starts hidden; started with Windows it stays that way
            if !background {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_drives,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_summary,
            commands::get_suggestions,
            commands::get_dir,
            commands::get_big_files,
            commands::get_duplicates,
            commands::reveal,
            commands::search,
            diag::get_app_info,
            diag::get_diagnostics,
            diag::log_ui,
            diag::open_url,
            cleanup::clean_suggestion,
            cleanup::quarantine_paths,
            cleanup::clean_duplicates,
            cleanup::get_history,
            cleanup::dismiss_history_notice,
            cleanup::synced_among,
            cleanup::restore_batch,
            cleanup::purge_batch,
            cleanup::empty_trash,
            settings::get_settings,
            settings::save_settings,
            settings::ignore_path,
            settings::is_admin,
            settings::relaunch_as_admin,
            settings::get_palette,
            background::get_background_status,
            background::run_background_scan_now,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Sift")
        .run(|app, event| {
            // last window closed: with background scans on, stay in the tray
            if let tauri::RunEvent::ExitRequested { api, code: None, .. } = event {
                if background::enabled(app) {
                    api.prevent_exit();
                }
            }
        });
}
