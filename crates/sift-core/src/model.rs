use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One folder in the scanned tree. Files are not stored individually (a full
/// drive has millions); only their totals, plus large files in `ScanResult`.
#[derive(Debug, Clone, Serialize)]
pub struct DirNode {
    pub name: String,
    /// Total bytes including all descendants.
    pub size: u64,
    /// Bytes in files directly inside this folder.
    pub own_bytes: u64,
    /// Files in this folder and below.
    pub file_count: u64,
    pub children: Vec<DirNode>,
    /// Set when a regenerable-folder rule matched this folder.
    pub regen_rule: Option<&'static str>,
    /// Folder could not be listed (permissions).
    pub denied: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    /// Unix seconds.
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub errors: u64,
    pub cloud_skipped: u64,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanResult {
    pub root_path: PathBuf,
    pub root: DirNode,
    /// Every file >= `ScanOptions::big_file_min`, largest first.
    pub big_files: Vec<FileEntry>,
    /// Bytes per file category ("Video", "Archives", ...).
    pub categories: Vec<(String, u64)>,
    /// Files matching a special-extension rule, by rule id.
    pub special: Vec<(&'static str, Vec<FileEntry>)>,
    pub stats: ScanStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Safety {
    /// Rebuilt automatically or pure junk.
    Safe,
    /// Check before removing.
    Review,
    /// Frees space but costs a feature.
    TradeOff,
    /// Informational; don't remove.
    Leave,
}

/// What the UI can do for a suggestion. Stage 3 executes these.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// Move the listed paths into Sift's quarantine (undoable). With
    /// `contents`, each listed folder stays and only what's inside moves
    /// (e.g. %TEMP% itself must keep existing).
    Quarantine { contents: bool },
    /// Run a program. Not undoable.
    Command { program: String, args: Vec<String> },
    /// Open a Windows Settings page, e.g. "ms-settings:storagesense".
    OpenSettings { uri: String },
    /// Only instructions; the user does it.
    Manual,
}

#[derive(Debug, Clone, Serialize)]
pub struct Located {
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub id: String,
    pub title: String,
    pub bytes: u64,
    pub safety: Safety,
    pub how: String,
    pub action: Action,
    pub items: Vec<Located>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub size: u64,
    pub wasted: u64,
    /// Oldest first.
    pub paths: Vec<PathBuf>,
    /// Unix seconds, parallel to `paths`.
    pub modified: Vec<i64>,
}
