use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Rule ids are `&'static str` so the tree stays small; when a saved scan is
/// read back they're looked up again (an id no rule has any more is dropped).
fn static_id(id: &str) -> Option<&'static str> {
    use crate::rules::{REGEN_RULES, SPECIAL_EXTS};
    REGEN_RULES.iter().map(|r| r.id).chain(SPECIAL_EXTS.iter().map(|r| r.id)).find(|r| *r == id)
}

#[derive(Deserialize)]
struct DirNodeDe {
    name: String,
    size: u64,
    own_bytes: u64,
    file_count: u64,
    #[serde(default)]
    children: Vec<DirNode>,
    #[serde(default)]
    regen_rule: Option<String>,
    #[serde(default)]
    denied: bool,
}

impl<'de> Deserialize<'de> for DirNode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = DirNodeDe::deserialize(d)?;
        let regen_rule = n.regen_rule.and_then(|s| static_id(&s));
        Ok(DirNode { name: n.name, size: n.size, own_bytes: n.own_bytes, file_count: n.file_count, children: n.children, regen_rule, denied: n.denied })
    }
}

#[derive(Deserialize)]
struct ScanResultDe {
    root_path: PathBuf,
    #[serde(default)]
    scanned_at: i64,
    root: DirNode,
    big_files: Vec<FileEntry>,
    categories: Vec<(String, u64)>,
    special: Vec<(String, Vec<FileEntry>)>,
    stats: ScanStats,
}

impl<'de> Deserialize<'de> for ScanResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = ScanResultDe::deserialize(d)?;
        let special = r.special.into_iter().filter_map(|(id, f)| static_id(&id).map(|id| (id, f))).collect();
        Ok(ScanResult { root_path: r.root_path, scanned_at: r.scanned_at, root: r.root, big_files: r.big_files, categories: r.categories, special, stats: r.stats })
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<DirNode>,
    /// Set when a regenerable-folder rule matched this folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regen_rule: Option<&'static str>,
    /// Folder could not be listed (permissions).
    #[serde(skip_serializing_if = "is_false")]
    pub denied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    /// Unix seconds.
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Unix seconds when the scan finished.
    pub scanned_at: i64,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Located {
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub title: String,
    pub bytes: u64,
    pub safety: Safety,
    pub how: String,
    pub action: Action,
    /// Carrying it out can't be undone from Sift (a command, Windows
    /// Settings, emptying a bin). The UI flags these loudly.
    #[serde(default)]
    pub permanent: bool,
    pub items: Vec<Located>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub size: u64,
    pub wasted: u64,
    /// Oldest first.
    pub paths: Vec<PathBuf>,
    /// Unix seconds, parallel to `paths`.
    pub modified: Vec<i64>,
}
