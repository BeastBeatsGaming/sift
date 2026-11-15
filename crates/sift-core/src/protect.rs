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
