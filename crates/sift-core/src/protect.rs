//! Guard rails: paths Sift refuses to move, whatever the UI asks for.

use std::path::{Component, Path, PathBuf};

pub const TRASH_DIR: &str = ".sift-trash";

fn norm(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.trim_end_matches('\\').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// `a` is `b` or contains `b`.
fn is_ancestor_or_same(a: &Path, b: &Path) -> bool {
    let (a, b) = (norm(a), norm(b));
    b == a || b.starts_with(&(a + "\\"))
}

/// Folders inside the Windows directory that are fine to clear.
const WINDOWS_ALLOWED: &[&str] = &["Temp", "Minidump", "LiveKernelReports", "MEMORY.DMP", "SoftwareDistribution\\Download"];

/// Ok if Sift may move `path` to its trash.
pub fn check_movable(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Not an absolute path".into());
    }
    // drive roots / filesystem root
    let normal = path.components().filter(|c| matches!(c, Component::Normal(_))).count();
    if normal == 0 {
        return Err("Refusing to move a drive root".into());
    }
    if path.components().any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case(TRASH_DIR)) {
        return Err("Already in .sift-trash".into());
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if matches!(name.as_str(), "pagefile.sys" | "hiberfil.sys" | "swapfile.sys" | "bootmgr" | "$recycle.bin" | "system volume information") {
        return Err("Windows system file — managed by Windows".into());
    }

    // Never move these folders or anything that contains them.
    for var in ["SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "HOME"] {
        if let Some(p) = env_path(var) {
            if is_ancestor_or_same(path, &p) {
                return Err(format!("Refusing to move {} or a folder containing it", p.display()));
            }
        }
    }
    // Inside the Windows folder, only known junk.
    if let Some(win) = env_path("SystemRoot") {
        if is_ancestor_or_same(&win, path) && !WINDOWS_ALLOWED.iter().any(|a| is_ancestor_or_same(&win.join(a), path)) {
            return Err("Inside the Windows folder — only temp, dumps and update caches can be moved".into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Installed software and cloud-synced folders
//
// `check_movable` only stops Sift moving the big system folders themselves.
// Suggestions built from the scan (stale large files, duplicates, `.dmp`
// files, `node_modules`, …) can still land *inside* them: Node's own npm
// under Program Files, a WSL `ext4.vhdx` under AppData, a game's data under
// `steamapps\common`. Moving one of those breaks the program while looking
// like routine cleanup, so scan-derived suggestions never include them.
// (Known cache locations like the Steam shader cache are exempt: they are
// listed by name in `rules.rs`, not discovered.)
// ---------------------------------------------------------------------------

/// Why a path is off-limits to scan-derived suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guarded {
    ProgramFiles,
    AppData,
    GameLibrary,
}

impl Guarded {
    pub fn reason(self) -> &'static str {
        match self {
            Guarded::ProgramFiles => "Part of an installed program — moving it would break the program",
            Guarded::AppData => "Inside AppData, where programs keep their data (WSL, Docker, saves) — not a safe thing to suggest",
            Guarded::GameLibrary => "Part of an installed game — moving it would break the game",
        }
    }
}

/// Folders (from the environment) that hold installed software or app data.
pub fn software_roots() -> Vec<(PathBuf, Guarded)> {
    let mut v = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"] {
        if let Some(p) = env_path(var) {
            v.push((p, Guarded::ProgramFiles));
        }
    }
    for var in ["APPDATA", "LOCALAPPDATA"] {
        if let Some(p) = env_path(var) {
            v.push((p, Guarded::AppData));
        }
    }
    v
}

/// Is `path` inside installed software? `roots` come from `software_roots()`
/// (plus any extra ones the caller wants guarded). Game libraries are
/// recognised by name wherever they sit, since they often live on another drive.
pub fn guarded_in(path: &Path, roots: &[(PathBuf, Guarded)]) -> Option<Guarded> {
    for (root, why) in roots {
        if is_ancestor_or_same(root, path) {
            return Some(*why);
        }
    }
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    for (i, p) in parts.iter().enumerate() {
        match p.as_str() {
            "steamapps" if matches!(parts.get(i + 1).map(String::as_str), Some("common" | "workshop" | "compatdata")) => {
                return Some(Guarded::GameLibrary)
            }
            "windowsapps" | "xboxgames" | "epic games" => return Some(Guarded::GameLibrary),
            _ => {}
        }
    }
    None
}

/// `guarded_in` against the machine's real folders.
pub fn guarded(path: &Path) -> Option<Guarded> {
    guarded_in(path, &software_roots())
}

/// OneDrive's sync roots: the ones Windows reports, plus any
/// `~\OneDrive*` folder (business accounts are `OneDrive - <org>`).
pub fn onedrive_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"].iter().filter_map(|k| env_path(k)).collect();
    if let Some(home) = env_path("USERPROFILE").or_else(|| env_path("HOME")) {
        if let Ok(rd) = std::fs::read_dir(&home) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().to_lowercase().starts_with("onedrive") && e.path().is_dir() {
                    v.push(e.path());
                }
            }
        }
    }
    v.sort();
    v.dedup();
    v
}

pub fn in_synced(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|r| is_ancestor_or_same(r, path))
}

/// Shown when a synced file is skipped.
pub const SYNCED_REASON: &str = "In OneDrive — moving it deletes it from your cloud storage too";
