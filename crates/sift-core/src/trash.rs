//! `.sift-trash`: undoable cleanup.
//!
//! Items are *renamed* into `<drive>\.sift-trash\<batch>\<n>\<name>` on the
//! same drive, so moving a 10 GB folder is instant and Undo is exact. Space is
//! freed only when a batch is purged. Every batch also writes a
//! `manifest.json` next to its files, so the trash explains itself even if
//! Sift's history file is lost.

use crate::protect::{check_movable, TRASH_DIR};
use crate::scan::path_size;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryState {
    InTrash,
    Restored,
    Purged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub original: PathBuf,
    pub stored: PathBuf,
    pub bytes: u64,
    pub state: EntryState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BatchKind {
    Quarantine { entries: Vec<Entry> },
    Command { program: String, args: Vec<String>, exit_code: Option<i32>, output: String, freed: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Batch {
    pub id: String,
    /// Unix seconds.
    pub created: i64,
    pub label: String,
    #[serde(flatten)]
    pub kind: BatchKind,
}

impl Batch {
    pub fn bytes_in_trash(&self) -> u64 {
        match &self.kind {
            BatchKind::Quarantine { entries } => entries.iter().filter(|e| e.state == EntryState::InTrash).map(|e| e.bytes).sum(),
            BatchKind::Command { .. } => 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct History {
    pub batches: Vec<Batch>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Skip {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Outcome {
    pub batch_id: Option<String>,
    pub done: usize,
    pub bytes: u64,
    pub skipped: Vec<Skip>,
    /// Original paths that were moved (for updating the scan view).
    #[serde(skip)]
    pub moved: Vec<(PathBuf, u64)>,
}

pub struct Trash {
    history_path: PathBuf,
    /// Used when a drive root isn't writable, and always off Windows.
    fallback_root: PathBuf,
    history: History,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn new_id() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{}-{:04x}", now(), (n % 0xffff) as u16)
}

/// `C:\foo\bar` → `C:\`. None off Windows or for relative paths.
fn volume_root(p: &Path) -> Option<PathBuf> {
    let mut it = p.components();
    match (it.next(), it.next()) {
        (Some(Component::Prefix(pre)), Some(Component::RootDir)) => {
            let mut r = PathBuf::from(pre.as_os_str());
            r.push("\\");
            Some(r)
        }
        _ => None,
    }
}

#[cfg(windows)]
fn hide(p: &Path) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("attrib").arg("+h").arg(p).creation_flags(0x0800_0000).status();
}
#[cfg(not(windows))]
fn hide(_: &Path) {}

/// Delete a file or folder, clearing read-only flags if the first try fails
/// (git object files are read-only on Windows).
pub fn force_remove(p: &Path) -> io::Result<()> {
    let m = match fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    let first = if m.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) };
    if first.is_ok() {
        return Ok(());
    }
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(meta) = fs::symlink_metadata(&d) {
            let mut perm = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(&d, perm);
            if meta.is_dir() {
                if let Ok(rd) = fs::read_dir(&d) {
                    stack.extend(rd.flatten().map(|e| e.path()));
                }
            }
        }
    }
    if m.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) }
}

fn friendly(e: &io::Error) -> String {
    match e.raw_os_error() {
        Some(32) | Some(33) => "In use by another program".into(),
        Some(5) => "Access denied (try running as administrator)".into(),
        Some(17) | Some(18) => "On a different drive than its .sift-trash".into(),
        _ => match e.kind() {
            io::ErrorKind::NotFound => "No longer exists".into(),
            io::ErrorKind::PermissionDenied => "Access denied".into(),
            _ => e.to_string(),
        },
    }
}

impl Trash {
    pub fn open(history_path: PathBuf, fallback_root: PathBuf) -> Self {
        let history = fs::read(&history_path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { history_path, fallback_root, history }
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn bytes_in_trash(&self) -> u64 {
        self.history.batches.iter().map(|b| b.bytes_in_trash()).sum()
    }

    fn save(&self) {
        if let Some(dir) = self.history_path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let tmp = self.history_path.with_extension("json.tmp");
        if let Ok(json) = serde_json::to_vec_pretty(&self.history) {
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.history_path);
            }
        }
    }

    fn write_manifest(&self, batch: &Batch) {
        if let BatchKind::Quarantine { entries } = &batch.kind {
            let mut dirs: Vec<PathBuf> = entries.iter().filter_map(|e| e.stored.parent()?.parent().map(Path::to_path_buf)).collect();
            dirs.sort();
            dirs.dedup();
            for d in dirs {
                let _ = fs::write(d.join("manifest.json"), serde_json::to_vec_pretty(batch).unwrap_or_default());
            }
        }
    }

    /// The trash folder for the drive holding `p`, created on demand.
    fn root_for(&self, p: &Path) -> PathBuf {
        if let Some(vol) = volume_root(p) {
            let t = vol.join(TRASH_DIR);
            if t.is_dir() {
                return t;
            }
            if fs::create_dir(&t).is_ok() {
                hide(&t);
                let _ = fs::write(
                    t.join("README.txt"),
                    "Files moved here by Sift can be restored from Sift's History tab.\r\nEach folder is one cleanup; manifest.json lists where everything came from.\r\nDeleting this folder frees the space permanently.\r\n",
                );
                return t;
            }
        }
        self.fallback_root.clone()
    }

    /// Move `targets` (path, bytes) into a new batch. With `contents`, each
    /// target folder stays in place and its children move instead.
    pub fn quarantine(&mut self, label: &str, targets: &[(PathBuf, u64)], contents: bool) -> Outcome {
        let mut out = Outcome::default();
        // expand to the concrete things that will move
        let mut work: Vec<(PathBuf, Option<u64>)> = Vec::new();
        for (p, bytes) in targets {
            if contents && p.is_dir() {
                match fs::read_dir(p) {
                    Ok(rd) => work.extend(rd.flatten().map(|e| (e.path(), None))),
                    Err(e) => out.skipped.push(Skip { path: p.clone(), reason: friendly(&e) }),
                }
            } else {
                work.push((p.clone(), Some(*bytes)));
            }
        }

        let id = new_id();
        let mut entries = Vec::new();
        for (i, (src, known)) in work.into_iter().enumerate() {
            if let Err(reason) = check_movable(&src) {
                out.skipped.push(Skip { path: src, reason });
                continue;
            }
            if fs::symlink_metadata(&src).is_err() {
                out.skipped.push(Skip { path: src, reason: "No longer exists".into() });
                continue;
            }
            let bytes = known.unwrap_or_else(|| path_size(&src).unwrap_or(0));
            let slot = self.root_for(&src).join(&id).join(i.to_string());
            if let Err(e) = fs::create_dir_all(&slot) {
                out.skipped.push(Skip { path: src, reason: friendly(&e) });
                continue;
            }
            let dest = slot.join(src.file_name().unwrap_or_default());
            match fs::rename(&src, &dest) {
                Ok(()) => {
                    out.done += 1;
                    out.bytes += bytes;
                    out.moved.push((src.clone(), bytes));
                    entries.push(Entry { original: src, stored: dest, bytes, state: EntryState::InTrash });
                }
                Err(e) => {
                    let _ = fs::remove_dir(&slot);
                    out.skipped.push(Skip { path: src, reason: friendly(&e) });
                }
            }
        }
        if !entries.is_empty() {
            let batch = Batch { id: id.clone(), created: now(), label: label.into(), kind: BatchKind::Quarantine { entries } };
            self.write_manifest(&batch);
            self.history.batches.push(batch);
            self.save();
            out.batch_id = Some(id);
        }
        out
    }

    /// Put a batch's items back where they came from. Items whose original
    /// location is occupied again (e.g. `npm install` recreated
    /// node_modules) stay in the trash and are reported.
    pub fn restore(&mut self, batch_id: &str) -> Result<Outcome, String> {
        let mut out = Outcome { batch_id: Some(batch_id.into()), ..Default::default() };
        let batch = self.history.batches.iter_mut().find(|b| b.id == batch_id).ok_or("Unknown batch")?;
        let BatchKind::Quarantine { entries } = &mut batch.kind else { return Err("Commands can't be undone".into()) };
        for e in entries.iter_mut().filter(|e| e.state == EntryState::InTrash) {
            if fs::symlink_metadata(&e.original).is_ok() {
                out.skipped.push(Skip { path: e.original.clone(), reason: "Something new exists at the original location".into() });
                continue;
            }
            if let Some(parent) = e.original.parent() {
                let _ = fs::create_dir_all(parent);
            }
            match fs::rename(&e.stored, &e.original) {
                Ok(()) => {
                    e.state = EntryState::Restored;
                    out.done += 1;
                    out.bytes += e.bytes;
                    if let Some(slot) = e.stored.parent() {
                        let _ = fs::remove_dir(slot);
                    }
                }
                Err(err) => out.skipped.push(Skip { path: e.original.clone(), reason: friendly(&err) }),
            }
        }
        let b = batch.clone();
        self.cleanup_batch_dirs(&b);
        self.write_manifest(&b);
        self.save();
        Ok(out)
    }

    /// Permanently delete a batch's remaining items.
    pub fn purge(&mut self, batch_id: &str) -> Result<Outcome, String> {
        let mut out = Outcome { batch_id: Some(batch_id.into()), ..Default::default() };
        let batch = self.history.batches.iter_mut().find(|b| b.id == batch_id).ok_or("Unknown batch")?;
        if let BatchKind::Quarantine { entries } = &mut batch.kind {
            for e in entries.iter_mut().filter(|e| e.state == EntryState::InTrash) {
                match force_remove(&e.stored) {
                    Ok(()) => {
                        e.state = EntryState::Purged;
                        out.done += 1;
                        out.bytes += e.bytes;
                    }
                    Err(err) => out.skipped.push(Skip { path: e.stored.clone(), reason: friendly(&err) }),
                }
            }
        }
        let b = batch.clone();
        self.cleanup_batch_dirs(&b);
        self.save();
        Ok(out)
    }

    pub fn purge_all(&mut self) -> Outcome {
        let ids: Vec<String> = self.history.batches.iter().filter(|b| b.bytes_in_trash() > 0).map(|b| b.id.clone()).collect();
        self.purge_ids(&ids)
    }

    /// Auto-cleanup: purge batches older than `days`.
    pub fn purge_older_than(&mut self, days: i64) -> Outcome {
        let cutoff = now() - days * 86_400;
        let ids: Vec<String> = self.history.batches.iter().filter(|b| b.created < cutoff && b.bytes_in_trash() > 0).map(|b| b.id.clone()).collect();
        self.purge_ids(&ids)
    }

    fn purge_ids(&mut self, ids: &[String]) -> Outcome {
        let mut total = Outcome::default();
        for id in ids {
            if let Ok(o) = self.purge(id) {
                total.done += o.done;
                total.bytes += o.bytes;
                total.skipped.extend(o.skipped);
            }
        }
        total
    }

    /// Remove empty slot folders and the batch folder (with its manifest)
    /// once nothing of the batch is left in the trash.
    fn cleanup_batch_dirs(&self, batch: &Batch) {
        let BatchKind::Quarantine { entries } = &batch.kind else { return };
        let mut batch_dirs: Vec<PathBuf> = Vec::new();
        for e in entries {
            if let Some(slot) = e.stored.parent() {
                if e.state != EntryState::InTrash {
                    let _ = fs::remove_dir(slot);
                }
                if let Some(bd) = slot.parent() {
                    batch_dirs.push(bd.to_path_buf());
                }
            }
        }
        batch_dirs.sort();
        batch_dirs.dedup();
        if batch.bytes_in_trash() == 0 && entries.iter().all(|e| e.state != EntryState::InTrash) {
            for bd in batch_dirs {
                let _ = fs::remove_file(bd.join("manifest.json"));
                let _ = fs::remove_dir(&bd);
            }
        }
    }

    pub fn record_command(&mut self, label: &str, program: &str, args: &[String], exit_code: Option<i32>, output: String, freed: u64) -> String {
        let id = new_id();
        self.history.batches.push(Batch {
            id: id.clone(),
            created: now(),
            label: label.into(),
            kind: BatchKind::Command { program: program.into(), args: args.to_vec(), exit_code, output, freed },
        });
        self.save();
        id
    }
}
