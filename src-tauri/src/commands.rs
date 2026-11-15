//! Commands the React UI calls through `invoke()`.

use serde::Serialize;
use sift_core::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

/// Items per suggestion sent to the UI; the full list stays in Rust.
const ITEM_CAP: usize = 300;
/// Children per folder sent to the explorer.
const CHILD_CAP: usize = 400;

pub struct Loaded {
    pub scan: ScanResult,
    pub report: Report,
    pub disk: Option<(u64, u64)>, // (total, free)
}

#[derive(Default)]
pub struct AppState {
    running: Mutex<Option<Arc<Progress>>>,
    pub data: RwLock<Option<Loaded>>,
    /// Opened in `setup` once the app data folder is known.
    pub trash: Mutex<Option<Trash>>,
    pub settings: Mutex<crate::settings::Settings>,
}

// ------------------------------------------------------------------ drives

#[derive(Serialize)]
pub struct DriveInfo {
    path: String,
    name: String,
    total: u64,
    free: u64,
}

fn disks() -> Vec<DriveInfo> {
    let Ok(list) = sysinfo::Disks::new_with_refreshed_list() else { return Vec::new() };
    let mut out: Vec<DriveInfo> = list
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| DriveInfo {
            path: d.mount_point().to_string_lossy().into_owned(),
            name: d.name().to_string_lossy().into_owned(),
            total: d.total_space(),
            free: d.available_space(),
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

#[tauri::command]
pub fn list_drives() -> Vec<DriveInfo> {
    disks()
}

/// (total, free) of the drive holding `path`: the longest matching mount point.
pub fn disk_for(path: &Path) -> Option<(u64, u64)> {
    let p = path.to_string_lossy().to_lowercase();
    disks()
        .into_iter()
        .filter(|d| p.starts_with(&d.path.to_lowercase()))
        .max_by_key(|d| d.path.len())
        .map(|d| (d.total, d.free))
}

// -------------------------------------------------------------------- scan

#[derive(Clone, Serialize)]
struct ProgressEvent {
    files: u64,
    dirs: u64,
    bytes: u64,
    current: String,
    phase: &'static str,
}

fn snapshot(p: &Progress, phase: &'static str) -> ProgressEvent {
    ProgressEvent {
        files: p.files.load(Relaxed),
        dirs: p.dirs.load(Relaxed),
        bytes: p.bytes.load(Relaxed),
        current: p.current.lock().map(|c| c.clone()).unwrap_or_default(),
        phase,
    }
}

#[tauri::command]
pub fn start_scan(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("Not a folder: {path}"));
    }
    let progress = {
        let mut running = state.running.lock().unwrap();
        if running.is_some() {
            return Err("A scan is already running".into());
        }
        let p = Arc::new(Progress::default());
        *running = Some(p.clone());
        p
    };

    thread::spawn(move || {
        let done = Arc::new(AtomicBool::new(false));
        let ticker = {
            let (app, p, done) = (app.clone(), progress.clone(), done.clone());
            thread::spawn(move || {
                while !done.load(Relaxed) {
                    let _ = app.emit("scan://progress", snapshot(&p, "scanning"));
                    thread::sleep(Duration::from_millis(150));
                }
            })
        };

        let scan = sift_core::scan(&root, &ScanOptions::default(), &progress);
        done.store(true, Relaxed);
        let _ = ticker.join();

        let state = app.state::<AppState>();
        if progress.is_cancelled() {
            *state.running.lock().unwrap() = None;
            let _ = app.emit("scan://cancelled", ());
            return;
        }

        let _ = app.emit("scan://progress", snapshot(&progress, "analyzing"));
        let report = suggest(&scan, &crate::settings::current(&app).suggest_options());
        let disk = disk_for(&root);
        *state.data.write().unwrap() = Some(Loaded { scan, report, disk });
        *state.running.lock().unwrap() = None;
        let _ = app.emit("scan://done", ());
    });
    Ok(())
}

use tauri::Manager;

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    if let Some(p) = state.running.lock().unwrap().as_ref() {
        p.cancel();
    }
}

// ----------------------------------------------------------------- results

#[derive(Serialize)]
pub struct Summary {
    root_path: PathBuf,
    total: u64,
    stats: ScanStats,
    categories: Vec<(String, u64)>,
    disk_total: Option<u64>,
    disk_free: Option<u64>,
    duplicate_groups: usize,
}

#[tauri::command]
pub fn get_summary(state: State<'_, AppState>) -> Result<Summary, String> {
    let g = state.data.read().unwrap();
    let l = g.as_ref().ok_or("No scan yet")?;
    Ok(Summary {
        root_path: l.scan.root_path.clone(),
        total: l.scan.root.size,
        stats: l.scan.stats.clone(),
        categories: l.scan.categories.clone(),
        disk_total: l.disk.map(|d| d.0),
        disk_free: l.disk.map(|d| d.1),
        duplicate_groups: l.report.duplicates.len(),
    })
}

#[derive(Serialize)]
pub struct SuggestionView {
    #[serde(flatten)]
    s: Suggestion,
    item_count: usize,
}

#[tauri::command]
pub fn get_suggestions(state: State<'_, AppState>) -> Result<Vec<SuggestionView>, String> {
    let g = state.data.read().unwrap();
    let l = g.as_ref().ok_or("No scan yet")?;
    Ok(l.report
        .suggestions
        .iter()
        .map(|s| {
            let mut s = s.clone();
            let item_count = s.items.len();
            s.items.truncate(ITEM_CAP);
            SuggestionView { s, item_count }
        })
        .collect())
}

#[derive(Serialize)]
pub struct ChildView {
    index: u32,
    name: String,
    size: u64,
    file_count: u64,
    regen_rule: Option<&'static str>,
    has_children: bool,
    denied: bool,
}

#[derive(Serialize)]
pub struct DirView {
    full_path: PathBuf,
    crumbs: Vec<String>,
    size: u64,
    own_bytes: u64,
    file_count: u64,
    regen_rule: Option<&'static str>,
    children: Vec<ChildView>,
    child_count: usize,
}

/// Folder at `path`, given as child indices from the root (stable for one scan).
#[tauri::command]
pub fn get_dir(state: State<'_, AppState>, path: Vec<u32>) -> Result<DirView, String> {
    let g = state.data.read().unwrap();
    let l = g.as_ref().ok_or("No scan yet")?;
    let mut node = &l.scan.root;
    let mut full = l.scan.root_path.clone();
    let mut crumbs = vec![l.scan.root_path.to_string_lossy().into_owned()];
    for i in path {
        node = node.children.get(i as usize).ok_or("Folder no longer exists")?;
        full.push(&node.name);
        crumbs.push(node.name.clone());
    }
    Ok(DirView {
        full_path: full,
        crumbs,
        size: node.size,
        own_bytes: node.own_bytes,
        file_count: node.file_count,
        regen_rule: node.regen_rule,
        child_count: node.children.len(),
        children: node
            .children
            .iter()
            .take(CHILD_CAP)
            .enumerate()
            .map(|(i, c)| ChildView {
                index: i as u32,
                name: c.name.clone(),
                size: c.size,
                file_count: c.file_count,
                regen_rule: c.regen_rule,
                has_children: !c.children.is_empty(),
                denied: c.denied,
            })
            .collect(),
    })
}

#[tauri::command]
pub fn get_big_files(state: State<'_, AppState>, limit: usize) -> Result<Vec<FileEntry>, String> {
    let g = state.data.read().unwrap();
    let l = g.as_ref().ok_or("No scan yet")?;
    Ok(l.scan.big_files.iter().take(limit).cloned().collect())
}

#[tauri::command]
pub fn get_duplicates(state: State<'_, AppState>) -> Result<Vec<DuplicateGroup>, String> {
    let g = state.data.read().unwrap();
    Ok(g.as_ref().ok_or("No scan yet")?.report.duplicates.clone())
}

/// Show a file or folder in Explorer (selected).
#[tauri::command]
pub fn reveal(path: String) -> Result<(), String> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err("That path no longer exists".into());
    }
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg("-R").arg(&path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(p.parent().unwrap_or(p)).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}
