//! Stage 3: cleanup actions, history and undo.

use crate::commands::{disk_for, AppState};
use serde::{Deserialize, Serialize};
use sift_core::trash::{EntryState, Skip};
use sift_core::*;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

const SKIP_CAP: usize = 50;
const ENTRY_CAP: usize = 60;

#[derive(Serialize, Default)]
pub struct CleanResult {
    kind: &'static str,
    batch_id: Option<String>,
    done: usize,
    bytes: u64,
    skipped: Vec<Skip>,
    skipped_count: usize,
    exit_code: Option<i32>,
    output: Option<String>,
}

impl CleanResult {
    fn from_outcome(kind: &'static str, mut o: Outcome) -> Self {
        let skipped_count = o.skipped.len();
        o.skipped.truncate(SKIP_CAP);
        Self { kind, batch_id: o.batch_id, done: o.done, bytes: o.bytes, skipped: o.skipped, skipped_count, ..Default::default() }
    }
}

fn label_of(title: &str) -> String {
    title.split(" — ").next().unwrap_or(title).to_string()
}

fn with_trash<T>(app: &AppHandle, f: impl FnOnce(&mut Trash) -> T) -> Result<T, String> {
    let state = app.state::<AppState>();
    let mut g = state.trash.lock().unwrap();
    let t = g.as_mut().ok_or("Trash not ready")?;
    Ok(f(t))
}

/// Reflect moved paths in the loaded scan so views update without a rescan.
fn forget(app: &AppHandle, moved: &[(PathBuf, u64)], remeasure: &[&str]) {
    let state = app.state::<AppState>();
    let mut g = state.data.write().unwrap();
    if let Some(l) = g.as_mut() {
        for (p, b) in moved {
            l.scan.forget(p, *b);
        }
        let paths: Vec<PathBuf> = moved.iter().map(|(p, _)| p.clone()).collect();
        l.report.forget(&paths, remeasure);
        l.disk = disk_for(&l.scan.root_path);
    }
}

fn changed(app: &AppHandle) {
    let _ = app.emit("data://changed", ());
    crate::commands::persist(app);
}

/// Carry out a suggestion. `paths` limits it to some of its items; `None`
/// means all of them (including ones beyond what the UI was sent).
#[tauri::command]
pub async fn clean_suggestion(app: AppHandle, id: String, paths: Option<Vec<String>>) -> Result<CleanResult, String> {
    let log_id = id.clone();
    let r = tauri::async_runtime::spawn_blocking(move || {
        let sugg = {
            let state = app.state::<AppState>();
            let g = state.data.read().unwrap();
            let l = g.as_ref().ok_or("No scan yet")?;
            l.report.suggestions.iter().find(|s| s.id == id).cloned().ok_or("That suggestion no longer exists")?
        };
        let label = label_of(&sugg.title);

        match &sugg.action {
            Action::Quarantine { .. } if sugg.id == "duplicates" => {
                // keep the oldest copy of each group, verified byte for byte
                let picks: Vec<(PathBuf, Vec<PathBuf>)> = {
                    let state = app.state::<AppState>();
                    let g = state.data.read().unwrap();
                    let l = g.as_ref().ok_or("No scan yet")?;
                    l.report
                        .duplicates
                        .iter()
                        .map(|g| {
                            let others = g.paths[1..]
                                .iter()
                                .filter(|p| paths.as_ref().is_none_or(|sel| sel.iter().any(|s| PathBuf::from(s) == **p)))
                                .cloned()
                                .collect();
                            (g.paths[0].clone(), others)
                        })
                        .collect()
                };
                quarantine_duplicates(&app, picks)
            }
            Action::Quarantine { contents } => {
                let targets: Vec<(PathBuf, u64)> = sugg
                    .items
                    .iter()
                    .filter(|it| paths.as_ref().is_none_or(|sel| sel.iter().any(|p| PathBuf::from(p) == it.path)))
                    .map(|it| (it.path.clone(), it.bytes))
                    .collect();
                if targets.is_empty() {
                    return Err("Nothing selected".into());
                }
                let out = with_trash(&app, |t| t.quarantine(&label, &targets, *contents))?;
                let remeasure: Vec<&str> = if *contents { vec![id.as_str()] } else { vec![] };
                forget(&app, &out.moved, &remeasure);
                changed(&app);
                Ok(CleanResult::from_outcome("quarantine", out))
            }
            Action::Command { program, args } => {
                let (exit_code, output) = actions::run_command(program, args);
                forget(&app, &[], &[id.as_str()]);
                let after = {
                    let state = app.state::<AppState>();
                    let g = state.data.read().unwrap();
                    g.as_ref().and_then(|l| l.report.suggestions.iter().find(|s| s.id == id).map(|s| s.bytes)).unwrap_or(0)
                };
                let freed = sugg.bytes.saturating_sub(after);
                let batch_id = with_trash(&app, |t| t.record_command(&label, program, args, exit_code, output.clone(), freed))?;
                changed(&app);
                Ok(CleanResult { kind: "command", batch_id: Some(batch_id), done: 1, bytes: freed, exit_code, output: Some(output), ..Default::default() })
            }
            Action::OpenSettings { uri } => {
                actions::open_settings(uri)?;
                Ok(CleanResult { kind: "settings", ..Default::default() })
            }
            Action::Manual => Err("This one needs doing by hand — see the instructions".into()),
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::diag::note(&format!("clean_suggestion {log_id}"), r)
}

/// Move arbitrary paths (from Explorer, Large files, Duplicates) to the trash.
#[tauri::command]
pub async fn quarantine_paths(app: AppHandle, paths: Vec<String>, label: String) -> Result<CleanResult, String> {
    let r = tauri::async_runtime::spawn_blocking(move || {
        let targets: Vec<(PathBuf, u64)> = {
            let state = app.state::<AppState>();
            let g = state.data.read().unwrap();
            paths
                .iter()
                .map(PathBuf::from)
                .map(|p| {
                    let known = g.as_ref().and_then(|l| {
                        l.scan.big_files.iter().find(|f| f.path == p).map(|f| f.size).or_else(|| suggest::tree_size(&l.scan, &p))
                    });
                    let bytes = known.or_else(|| scan::path_size(&p)).unwrap_or(0);
                    (p, bytes)
                })
                .collect()
        };
        let out = with_trash(&app, |t| t.quarantine(&label, &targets, false))?;
        forget(&app, &out.moved, &[]);
        changed(&app);
        Ok(CleanResult::from_outcome("quarantine", out))
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::diag::note("quarantine_paths", r)
}

#[derive(Deserialize)]
pub struct DupePick {
    /// The copy to keep.
    keep: String,
    /// Copies to move (all other members of the group the user left ticked).
    remove: Vec<String>,
}

/// Move duplicate copies to the trash, keeping the chosen copy of each group.
/// Every copy is compared in full against its keeper first; mismatches stay.
#[tauri::command]
pub async fn clean_duplicates(app: AppHandle, picks: Vec<DupePick>) -> Result<CleanResult, String> {
    let r = tauri::async_runtime::spawn_blocking(move || {
        let picks: Vec<(PathBuf, Vec<PathBuf>)> = {
            let state = app.state::<AppState>();
            let g = state.data.read().unwrap();
            let l = g.as_ref().ok_or("No scan yet")?;
            let mut out = Vec::new();
            for p in picks {
                let keep = PathBuf::from(&p.keep);
                // only act within a group the scan found, and never on the keeper
                let group = l.report.duplicates.iter().find(|g| g.paths.contains(&keep)).ok_or("That duplicate group no longer exists")?;
                let remove: Vec<PathBuf> = p.remove.iter().map(PathBuf::from).filter(|r| *r != keep && group.paths.contains(r)).collect();
                if !remove.is_empty() {
                    out.push((keep, remove));
                }
            }
            out
        };
        quarantine_duplicates(&app, picks)
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::diag::note("clean_duplicates", r)
}

fn quarantine_duplicates(app: &AppHandle, picks: Vec<(PathBuf, Vec<PathBuf>)>) -> Result<CleanResult, String> {
    let mut targets: Vec<(PathBuf, u64)> = Vec::new();
    let mut rejected: Vec<Skip> = Vec::new();
    for (keep, others) in &picks {
        let (same, differ) = dupes::verify_copies(keep, others)?;
        for p in same {
            let bytes = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            targets.push((p, bytes));
        }
        rejected.extend(differ.into_iter().map(|(path, why)| Skip { path, reason: why.to_string() }));
    }
    if targets.is_empty() {
        if rejected.is_empty() {
            return Err("Nothing selected".into());
        }
        let mut r = CleanResult::from_outcome("quarantine", Outcome { skipped: rejected, ..Default::default() });
        r.done = 0;
        return Ok(r);
    }
    let mut out = with_trash(app, |t| t.quarantine("Duplicate files", &targets, false))?;
    forget(app, &out.moved, &[]);
    out.skipped.extend(rejected);
    changed(app);
    Ok(CleanResult::from_outcome("quarantine", out))
}

// ----------------------------------------------------------------- history

#[derive(Serialize)]
pub struct EntryView {
    original: PathBuf,
    bytes: u64,
    state: EntryState,
}

#[derive(Serialize)]
pub struct BatchView {
    id: String,
    created: i64,
    label: String,
    kind: &'static str,
    in_trash: u64,
    total: u64,
    entry_count: usize,
    /// Space this cleanup took off the drive: everything not restored (still
    /// in the trash or already purged), or what a command freed.
    freed: u64,
    restored: usize,
    purged: usize,
    entries: Vec<EntryView>,
    // commands
    command: Option<String>,
    exit_code: Option<i32>,
    output: Option<String>,
}

#[derive(Serialize)]
pub struct FreedPoint {
    created: i64,
    freed: u64,
}

#[derive(Serialize)]
pub struct HistoryView {
    in_trash: u64,
    auto_purge_days: i64,
    batches: Vec<BatchView>,
    /// Every cleanup ever (not capped like `batches`), for the freed totals.
    freed: Vec<FreedPoint>,
}

fn freed_by(b: &Batch) -> u64 {
    match &b.kind {
        BatchKind::Quarantine { entries } => entries.iter().filter(|e| e.state != EntryState::Restored).map(|e| e.bytes).sum(),
        BatchKind::Command { freed, .. } => *freed,
    }
}

fn view(b: &Batch) -> BatchView {
    match &b.kind {
        BatchKind::Quarantine { entries } => BatchView {
            id: b.id.clone(),
            created: b.created,
            label: b.label.clone(),
            kind: "quarantine",
            in_trash: b.bytes_in_trash(),
            total: entries.iter().map(|e| e.bytes).sum(),
            entry_count: entries.len(),
            freed: freed_by(b),
            restored: entries.iter().filter(|e| e.state == EntryState::Restored).count(),
            purged: entries.iter().filter(|e| e.state == EntryState::Purged).count(),
            entries: entries.iter().take(ENTRY_CAP).map(|e| EntryView { original: e.original.clone(), bytes: e.bytes, state: e.state }).collect(),
            command: None,
            exit_code: None,
            output: None,
        },
        BatchKind::Command { program, args, exit_code, output, freed } => BatchView {
            id: b.id.clone(),
            created: b.created,
            label: b.label.clone(),
            kind: "command",
            in_trash: 0,
            total: *freed,
            entry_count: 0,
            freed: *freed,
            restored: 0,
            purged: 0,
            entries: vec![],
            command: Some(format!("{program} {}", args.join(" "))),
            exit_code: *exit_code,
            output: Some(output.clone()),
        },
    }
}

#[tauri::command]
pub fn get_history(app: AppHandle) -> Result<HistoryView, String> {
    let auto_purge_days = crate::settings::current(&app).auto_purge_days;
    with_trash(&app, |t| HistoryView {
        in_trash: t.bytes_in_trash(),
        auto_purge_days,
        batches: t.history().batches.iter().rev().take(200).map(view).collect(),
        freed: t.history().batches.iter().map(|b| FreedPoint { created: b.created, freed: freed_by(b) }).filter(|p| p.freed > 0).collect(),
    })
}

#[tauri::command]
pub async fn restore_batch(app: AppHandle, id: String) -> Result<CleanResult, String> {
    let r = tauri::async_runtime::spawn_blocking(move || {
        let out = with_trash(&app, |t| t.restore(&id))??;
        // Restored items aren't in the loaded scan any more; the UI offers a rescan.
        let _ = app.emit("data://stale", ());
        changed(&app);
        Ok(CleanResult::from_outcome("restore", out))
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::diag::note("restore_batch", r)
}

#[tauri::command]
pub async fn purge_batch(app: AppHandle, id: String) -> Result<CleanResult, String> {
    let r = tauri::async_runtime::spawn_blocking(move || {
        let out = with_trash(&app, |t| t.purge(&id))??;
        refresh_disk(&app);
        changed(&app);
        Ok(CleanResult::from_outcome("purge", out))
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::diag::note("purge_batch", r)
}

#[tauri::command]
pub async fn empty_trash(app: AppHandle) -> Result<CleanResult, String> {
    let r = tauri::async_runtime::spawn_blocking(move || {
        let out = with_trash(&app, |t| t.purge_all())?;
        refresh_disk(&app);
        changed(&app);
        Ok(CleanResult::from_outcome("purge", out))
    })
    .await
    .map_err(|e| e.to_string())?;
    crate::diag::note("empty_trash", r)
}

fn refresh_disk(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut g = state.data.write().unwrap();
    if let Some(l) = g.as_mut() {
        l.disk = disk_for(&l.scan.root_path);
    }
}

/// Open the trash and empty batches older than the auto-purge setting.
pub fn init(app: &tauri::App, auto_purge_days: i64) -> Result<(), Box<dyn std::error::Error>> {
    let data = app.path().app_data_dir()?;
    let local = app.path().app_local_data_dir()?;
    let mut trash = Trash::open(data.join("history.json"), local.join("trash"));
    if auto_purge_days > 0 {
        trash.purge_older_than(auto_purge_days);
    }
    *app.state::<AppState>().trash.lock().unwrap() = Some(trash);
    Ok(())
}
