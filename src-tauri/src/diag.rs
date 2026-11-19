//! Diagnostics for test builds: a small rolling log file, a panic hook, and
//! a copyable summary for bug reports. Nothing leaves the machine unless the
//! user pastes it somewhere.

use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

pub const REPO: &str = "https://github.com/BeastBeatsGaming/sift";
/// Rotated to sift.old.log past this size, so at most ~2 MB is kept.
const MAX_LOG: u64 = 1024 * 1024;

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

/// UTC "YYYY-MM-DD HH:MM:SS" without a date crate.
fn stamp() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

pub fn write(level: &str, msg: &str) {
    let Some(path) = LOG_PATH.get() else { return };
    let _g = LOCK.lock();
    if fs::metadata(path).map(|m| m.len() > MAX_LOG).unwrap_or(false) {
        let _ = fs::rename(path, path.with_file_name("sift.old.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        // one line per entry keeps the log greppable
        let _ = writeln!(f, "{} {level:<5} {}", stamp(), msg.replace(['\r', '\n'], " ⏎ "));
    }
}

pub fn info(msg: impl AsRef<str>) {
    write("INFO", msg.as_ref());
}
pub fn warn(msg: impl AsRef<str>) {
    write("WARN", msg.as_ref());
}
pub fn error(msg: impl AsRef<str>) {
    write("ERROR", msg.as_ref());
}

pub fn version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

fn os() -> String {
    sysinfo::System::long_os_version().unwrap_or_else(|_| std::env::consts::OS.into())
}

/// Called once from `setup`: opens the log and records panics in it.
pub fn init(app: &tauri::App) {
    if let Ok(d) = app.path().app_log_dir() {
        let _ = fs::create_dir_all(&d);
        let _ = LOG_PATH.set(d.join("sift.log"));
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        error(format!("panic: {info}"));
        default(info);
    }));
    info(format!("start · Sift {} · {} · admin={}", version(app.handle()), os(), crate::settings::is_admin()));
}

pub fn log_path() -> Option<PathBuf> {
    LOG_PATH.get().cloned()
}

fn tail(lines: usize) -> String {
    let Some(p) = LOG_PATH.get() else { return String::new() };
    let text = fs::read_to_string(p).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// Log a cleanup command's result on its way back to the UI.
pub fn note<T: Serialize>(what: &str, r: Result<T, String>) -> Result<T, String> {
    match &r {
        Ok(v) => info(format!("{what} · {}", serde_json::to_string(v).map(|s| s.chars().take(400).collect::<String>()).unwrap_or_default())),
        Err(e) => warn(format!("{what} failed · {e}")),
    }
    r
}

// --------------------------------------------------------------- commands

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    os: String,
    repo: &'static str,
    log_path: Option<PathBuf>,
}

#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo { version: version(&app), os: os(), repo: REPO, log_path: log_path() }
}

/// Text for a bug report: versions, settings that change behaviour, the
/// loaded scan's numbers and the end of the log. No ignored paths.
#[tauri::command]
pub fn get_diagnostics(app: AppHandle) -> String {
    let s = crate::settings::current(&app);
    let state = app.state::<crate::commands::AppState>();
    let scan = state
        .data
        .read()
        .ok()
        .and_then(|g| {
            g.as_ref().map(|l| {
                let st = &l.scan.stats;
                format!(
                    "{} · {} files, {} folders, {} errors, {} cloud-only skipped, {} ms · {} suggestions",
                    l.scan.root_path.display(),
                    st.files,
                    st.dirs,
                    st.errors,
                    st.cloud_skipped,
                    st.elapsed_ms,
                    l.report.suggestions.len()
                )
            })
        })
        .unwrap_or_else(|| "none loaded".into());
    format!(
        "Sift {}\nOS: {}\nAdmin: {}\nSettings: background_scan={} rice_theme={} custom_titlebar={} find_duplicates={} auto_purge_days={} ignored={}\nScan: {}\n\n--- last log lines ---\n{}",
        version(&app),
        os(),
        crate::settings::is_admin(),
        s.background_scan,
        s.rice_theme,
        s.custom_titlebar,
        s.find_duplicates,
        s.auto_purge_days,
        s.ignored.len(),
        scan,
        tail(80)
    )
}

/// Errors from the webview (uncaught exceptions, failed calls).
#[tauri::command]
pub fn log_ui(level: String, message: String) {
    let msg: String = message.chars().take(2000).collect();
    write(if level == "error" { "ERROR" } else { "WARN" }, &format!("ui · {msg}"));
}

/// Open one of Sift's own GitHub pages in the default browser. Limited to
/// the repo so the webview can't be used to launch arbitrary URLs.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !(url == REPO || url.starts_with(&format!("{REPO}/"))) || url.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err("Only Sift's GitHub pages can be opened".into());
    }
    #[cfg(windows)]
    let r = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &url]).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(&url).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}
