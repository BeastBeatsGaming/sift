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

/// Mount points of every drive, for finding the `.sift-trash` folders.
pub fn drive_roots() -> Vec<PathBuf> {
    disks().into_iter().map(|d| PathBuf::from(d.path)).collect()
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

    crate::diag::info(format!("scan start · {path}"));
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
            crate::diag::info("scan cancelled");
            return;
        }

        let _ = app.emit("scan://progress", snapshot(&progress, "analyzing"));
        let report = suggest(&scan, &crate::settings::current(&app).suggest_options());
        let disk = disk_for(&root);
        let st = &scan.stats;
        crate::diag::info(format!(
            "scan done · {} · {} files, {} folders, {} errors · {} ms · {} suggestions",
            root.display(),
            st.files,
            st.dirs,
            st.errors,
            st.elapsed_ms,
            report.suggestions.len()
        ));
        *state.data.write().unwrap() = Some(Loaded { scan, report, disk });
        *state.running.lock().unwrap() = None;
        let _ = app.emit("scan://done", ());
        persist(&app);
    });
    Ok(())
}

use tauri::Manager;

// ------------------------------------------------------------- last scan

fn snapshot_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("last-scan.json"))
}

static SAVING: AtomicBool = AtomicBool::new(false);
static SAVE_AGAIN: AtomicBool = AtomicBool::new(false);

/// Save the loaded scan in the background (after a scan and after every
/// change), so the next launch opens straight to it. Calls that arrive while
/// a save is running are folded into one more save.
pub fn persist(app: &AppHandle) {
    if SAVING.swap(true, Relaxed) {
        SAVE_AGAIN.store(true, Relaxed);
        return;
    }
    let app = app.clone();
    thread::spawn(move || {
        loop {
            SAVE_AGAIN.store(false, Relaxed);
            if let Some(path) = snapshot_path(&app) {
                let state = app.state::<AppState>();
                let g = state.data.read().unwrap();
                if let Some(l) = g.as_ref() {
                    if let Err(e) = sift_core::snapshot::save(&path, &l.scan, &l.report) {
                        crate::diag::warn(format!("couldn't save last scan · {e}"));
                    }
                }
            }
            if !SAVE_AGAIN.load(Relaxed) {
                break;
            }
        }
        SAVING.store(false, Relaxed);
    });
}

/// Load the saved scan on start, unless a scan has already started or
/// something else (a background scan) got there first.
pub fn restore_last(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let Some(path) = snapshot_path(&app) else { return };
        let started = std::time::Instant::now();
        let Some((scan, report)) = sift_core::snapshot::load(&path) else { return };
        if !scan.root_path.is_dir() {
            return;
        }
        let state = app.state::<AppState>();
        if state.running.lock().unwrap().is_some() {
            return;
        }
        {
            let mut g = state.data.write().unwrap();
            if g.is_some() {
                return;
            }
            let disk = disk_for(&scan.root_path);
            crate::diag::info(format!("restored last scan · {} · {} ms", scan.root_path.display(), started.elapsed().as_millis()));
            *g = Some(Loaded { scan, report, disk });
        }
        let _ = app.emit("scan://done", ());
    });
}

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
    /// Unix seconds; older results come from the saved last scan.
    scanned_at: i64,
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
        scanned_at: l.scan.scanned_at,
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

// ------------------------------------------------------------------ search

#[derive(Serialize)]
pub struct SearchHit {
    path: PathBuf,
    size: u64,
    is_dir: bool,
    /// For folders: child indices for `get_dir`.
    index_path: Option<Vec<u32>>,
}

fn search_dirs(node: &DirNode, path: &mut PathBuf, idx: &mut Vec<u32>, q: &str, out: &mut Vec<SearchHit>) {
    for (i, c) in node.children.iter().enumerate() {
        path.push(&c.name);
        idx.push(i as u32);
        if c.name.to_lowercase().contains(q) {
            out.push(SearchHit { path: path.clone(), size: c.size, is_dir: true, index_path: Some(idx.clone()) });
        }
        search_dirs(c, path, idx, q, out);
        idx.pop();
        path.pop();
    }
}

/// Folders anywhere in the scan, and large files, whose name contains `query`.
/// Largest first. Small files aren't kept per file, so they can't be found.
#[tauri::command]
pub fn search(state: State<'_, AppState>, query: String, limit: usize) -> Result<Vec<SearchHit>, String> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let g = state.data.read().unwrap();
    let l = g.as_ref().ok_or("No scan yet")?;
    let mut out = Vec::new();
    search_dirs(&l.scan.root, &mut l.scan.root_path.clone(), &mut Vec::new(), &q, &mut out);
    for f in &l.scan.big_files {
        let name = f.path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if name.contains(&q) {
            out.push(SearchHit { path: f.path.clone(), size: f.size, is_dir: false, index_path: None });
        }
    }
    out.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    out.truncate(limit.min(500));
    Ok(out)
}
