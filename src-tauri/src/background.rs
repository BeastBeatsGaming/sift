//! Optional background scans: Sift stays in the tray, rescans on a schedule
//! and shows a Windows notification when safe-to-clear passes a threshold.
//!
//! With background scans on, Sift starts with Windows (hidden, `--background`)
//! and closing the window keeps it in the tray: the window (and its webview)
//! is destroyed to free memory and rebuilt from the tray.

use crate::commands::{disk_for, AppState, Loaded};
use serde::{Deserialize, Serialize};
use sift_core::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::OnceLock;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_notification::NotificationExt as _;

pub const BACKGROUND_ARG: &str = "--background";
const TRAY_ID: &str = "sift-tray";
const GB: u64 = 1024 * 1024 * 1024;

/// Persisted between runs, apart from settings so saving settings from the UI
/// can't overwrite it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Status {
    /// Unix seconds of the last finished background scan.
    pub last_run: i64,
    pub last_root: String,
    pub last_safe: u64,
    pub last_notified: bool,
}

static STATUS_PATH: OnceLock<PathBuf> = OnceLock::new();
static RUNNING: AtomicBool = AtomicBool::new(false);

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub fn load_status() -> Status {
    STATUS_PATH.get().and_then(|p| fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_status(s: &Status) {
    if let Some(p) = STATUS_PATH.get() {
        if let Some(d) = p.parent() {
            let _ = fs::create_dir_all(d);
        }
        let _ = fs::write(p, serde_json::to_vec_pretty(s).unwrap_or_default());
    }
}

/// Drive to scan when the setting is empty: the Windows drive.
pub fn default_root() -> String {
    std::env::var("SystemDrive").map(|d| format!("{d}\\")).unwrap_or_else(|_| "C:\\".into())
}

pub fn enabled(app: &AppHandle) -> bool {
    crate::settings::current(app).background_scan
}

// --------------------------------------------------------------- lifecycle

/// Called once from `setup`.
pub fn init(app: &tauri::App) {
    if let Ok(d) = app.path().app_data_dir() {
        let _ = STATUS_PATH.set(d.join("background.json"));
    }
    let handle = app.handle().clone();
    apply(&handle);
    std::thread::spawn(move || scheduler(handle));
}

/// Bring tray icon and start-with-Windows in line with the setting.
pub fn apply(app: &AppHandle) {
    let on = enabled(app);
    let auto = app.autolaunch();
    let _ = if on { auto.enable() } else { auto.disable() };
    if on {
        if app.tray_by_id(TRAY_ID).is_none() {
            if let Err(e) = build_tray(app) {
                eprintln!("tray: {e}");
            }
        }
    } else {
        let _ = app.remove_tray_by_id(TRAY_ID);
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Sift", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "Scan now", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Sift", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &scan, &quit])?;
    let mut b = TrayIconBuilder::with_id(TRAY_ID).tooltip(tooltip()).menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.on_menu_event(|app, e| match e.id().as_ref() {
        "open" => show_window(app),
        "scan" => {
            let app = app.clone();
            std::thread::spawn(move || run(&app, true));
        }
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, e| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            show_window(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

fn tooltip() -> String {
    let s = load_status();
    if s.last_run == 0 {
        "Sift".into()
    } else {
        format!("Sift — {} safe to clear on {}", human(s.last_safe), s.last_root)
    }
}

fn human(b: u64) -> String {
    if b >= GB {
        format!("{:.1} GB", b as f64 / GB as f64)
    } else {
        format!("{} MB", b / (1024 * 1024))
    }
}

/// Show the main window, rebuilding it if it was closed to the tray.
pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() else { return };
    match tauri::WebviewWindowBuilder::from_config(app, &cfg).and_then(|b| b.build()) {
        Ok(w) => {
            let _ = w.show();
            let _ = w.set_focus();
        }
        Err(e) => eprintln!("reopen window: {e}"),
    }
}

// --------------------------------------------------------------- schedule

fn due(app: &AppHandle) -> bool {
    let s = crate::settings::current(app);
    s.background_scan && now() - load_status().last_run >= (s.scan_every_days.max(1) as i64) * 86_400
}

fn scheduler(app: AppHandle) {
    // let login settle before the first check
    std::thread::sleep(Duration::from_secs(120));
    loop {
        if due(&app) {
            run(&app, false);
        }
        std::thread::sleep(Duration::from_secs(15 * 60));
    }
}

/// One background scan. `manual` (tray "Scan now") notifies even below the
/// threshold so the click gets an answer.
pub fn run(app: &AppHandle, manual: bool) {
    if RUNNING.swap(true, Relaxed) {
        return;
    }
    let settings = crate::settings::current(app);
    let root_s = if settings.scan_path.trim().is_empty() { default_root() } else { settings.scan_path.clone() };
    let root = PathBuf::from(&root_s);
    if !root.is_dir() {
        RUNNING.store(false, Relaxed);
        return;
    }
    let progress = Progress::default();
    let scan = sift_core::scan(&root, &ScanOptions::default(), &progress);
    let report = suggest(&scan, &settings.suggest_options());
    let safe: u64 = report.suggestions.iter().filter(|s| s.safety == Safety::Safe).map(|s| s.bytes).sum();
    let over = safe >= settings.notify_gb * GB;

    let state = app.state::<AppState>();
    // hand the result to the UI when nothing else is loaded, so opening Sift
    // from the notification shows it right away
    let window_open = app.get_webview_window("main").is_some();
    let mut stored = false;
    if over || window_open {
        let mut g = state.data.write().unwrap();
        if g.is_none() {
            let disk = disk_for(&root);
            *g = Some(Loaded { scan, report, disk });
            stored = true;
        }
    }
    if stored {
        let _ = app.emit("scan://done", ());
        crate::commands::persist(app);
    }
    crate::diag::info(format!("background scan · {root_s} · {} safe · notify={over}", human(safe)));

    save_status(&Status { last_run: now(), last_root: root_s.clone(), last_safe: safe, last_notified: over });
    if let Some(t) = app.tray_by_id(TRAY_ID) {
        let _ = t.set_tooltip(Some(tooltip()));
    }
    if over || manual {
        let body = if over {
            format!("{} is safe to clear on {}. Open Sift to review.", human(safe), root_s)
        } else {
            format!("{} is safe to clear on {} — below your {} GB alert.", human(safe), root_s, settings.notify_gb)
        };
        let _ = app.notification().builder().title("Sift").body(body).show();
    }
    RUNNING.store(false, Relaxed);
}

// --------------------------------------------------------------- commands

#[derive(Serialize)]
pub struct StatusView {
    last_run: i64,
    last_root: String,
    last_safe: u64,
    next_run: Option<i64>,
    default_root: String,
    running: bool,
}

#[tauri::command]
pub fn get_background_status(app: AppHandle) -> StatusView {
    let s = load_status();
    let settings = crate::settings::current(&app);
    StatusView {
        next_run: settings.background_scan.then(|| (s.last_run + settings.scan_every_days.max(1) as i64 * 86_400).max(now())),
        last_run: s.last_run,
        last_root: s.last_root,
        last_safe: s.last_safe,
        default_root: default_root(),
        running: RUNNING.load(Relaxed),
    }
}

#[tauri::command]
pub fn run_background_scan_now(app: AppHandle) -> Result<(), String> {
    if RUNNING.load(Relaxed) {
        return Err("A background scan is already running".into());
    }
    std::thread::spawn(move || run(&app, true));
    Ok(())
}
