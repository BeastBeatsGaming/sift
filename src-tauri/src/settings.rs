//! Stage 4: settings, ignore list, admin relaunch and rice palette theming.

use crate::commands::AppState;
use serde::{Deserialize, Serialize};
use sift_core::SuggestOptions;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

const MB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Days before a trash batch is emptied automatically; 0 = never.
    pub auto_purge_days: i64,
    pub min_suggestion_mb: u64,
    pub stale_days: i64,
    pub downloads_days: i64,
    pub find_duplicates: bool,
    pub duplicate_min_mb: u64,
    /// Paths never suggested (and everything inside them).
    pub ignored: Vec<String>,
    pub rice_theme: bool,
    pub custom_titlebar: bool,
    /// Stay in the tray, start with Windows and rescan on a schedule.
    pub background_scan: bool,
    pub scan_every_days: u32,
    /// Drive or folder for background scans; empty = the Windows drive.
    pub scan_path: String,
    /// Notify when safe-to-clear reaches this many GB.
    pub notify_gb: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_purge_days: 14,
            min_suggestion_mb: 50,
            stale_days: 365,
            downloads_days: 60,
            find_duplicates: true,
            duplicate_min_mb: 50,
            ignored: Vec::new(),
            rice_theme: true,
            custom_titlebar: true,
            background_scan: false,
            scan_every_days: 7,
            scan_path: String::new(),
            notify_gb: 10,
        }
    }
}

impl Settings {
    pub fn suggest_options(&self) -> SuggestOptions {
        SuggestOptions {
            min_bytes: self.min_suggestion_mb * MB,
            dup_min: self.duplicate_min_mb * MB,
            stale_days: self.stale_days,
            downloads_days: self.downloads_days,
            find_duplicates: self.find_duplicates,
            ignored: self.ignored.iter().map(PathBuf::from).collect(),
            ..SuggestOptions::default()
        }
    }
}

static PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn load(app: &tauri::App) -> Settings {
    let path = app.path().app_data_dir().map(|d| d.join("settings.json")).unwrap_or_else(|_| PathBuf::from("settings.json"));
    let s = fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let _ = PATH.set(path);
    s
}

fn save(s: &Settings) -> Result<(), String> {
    let path = PATH.get().ok_or("Settings path unknown")?;
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    fs::write(path, serde_json::to_vec_pretty(s).unwrap_or_default()).map_err(|e| e.to_string())
}

pub fn current(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    current(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    save(&settings)?;
    let (newly_ignored, background_changed): (Vec<PathBuf>, bool) = {
        let state = app.state::<AppState>();
        let mut g = state.settings.lock().unwrap();
        let added = settings.ignored.iter().filter(|p| !g.ignored.contains(p)).map(PathBuf::from).collect();
        let bg = g.background_scan != settings.background_scan;
        *g = settings;
        (added, bg)
    };
    apply_ignored(&app, &newly_ignored);
    if background_changed {
        crate::background::apply(&app);
    }
    Ok(())
}

/// Add one path to the ignore list (from a suggestion row).
#[tauri::command]
pub fn ignore_path(app: AppHandle, path: String) -> Result<(), String> {
    let mut s = current(&app);
    if !s.ignored.contains(&path) {
        s.ignored.push(path);
    }
    save_settings(app, s)
}

fn apply_ignored(app: &AppHandle, paths: &[PathBuf]) {
    if paths.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    if let Some(l) = state.data.write().unwrap().as_mut() {
        l.report.forget(paths, &[]);
    }
    let _ = app.emit("data://changed", ());
}

// ------------------------------------------------------------------- admin

#[tauri::command]
pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // `net session` only succeeds in an elevated process.
        std::process::Command::new("net")
            .arg("session")
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::env::var("USER").map(|u| u == "root").unwrap_or(false)
    }
}

/// Start an elevated copy of Sift (UAC prompt), then quit this one.
#[tauri::command]
pub fn relaunch_as_admin(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe = exe.to_string_lossy().replace('\'', "''");
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &format!("Start-Process -FilePath '{exe}' -ArgumentList '{}' -Verb RunAs", crate::ELEVATED_ARG)])
            .creation_flags(0x0800_0000)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Elevation was cancelled".into());
        }
        app.exit(0);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("Only available on Windows".into())
    }
}

// ------------------------------------------------------------------- theme

fn palette_path() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".config").join("rice").join("palette.json"))
}

#[derive(Deserialize)]
struct PaletteFile {
    colors: BTreeMap<String, String>,
}

/// Colour roles from the rice palette (`~/.config/rice/palette.json`).
#[tauri::command]
pub fn get_palette() -> Option<BTreeMap<String, String>> {
    let bytes = fs::read(palette_path()?).ok()?;
    let p: PaletteFile = serde_json::from_slice(&bytes).ok()?;
    Some(p.colors.into_iter().filter(|(_, v)| v.starts_with('#') && (v.len() == 7 || v.len() == 9)).collect())
}

/// Poll the palette file and tell the UI when the wallpaper changes it.
pub fn watch_palette(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: Option<SystemTime> = None;
        loop {
            let m = palette_path().and_then(|p| fs::metadata(p).ok()).and_then(|m| m.modified().ok());
            if m.is_some() && m != last {
                if last.is_some() {
                    let _ = app.emit("theme://changed", get_palette());
                }
                last = m;
            }
            std::thread::sleep(Duration::from_millis(1500));
        }
    });
}
