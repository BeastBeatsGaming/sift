//! `.sift-trash`: undoable cleanup.
//!
//! Items are *renamed* into `<drive>\.sift-trash\<batch>\<n>\<name>` on the
//! same drive, so moving a 10 GB folder is instant and Undo is exact. Space is
//! freed only when a batch is purged. Every batch also writes a
//! `manifest.json` next to its files, so the trash explains itself even if
//! Sift's history file is lost.
//!
//! A cleanup is journaled before it starts: the batch is written to the
//! history file (and the manifest) with every planned item marked `Pending`,
//! *then* the files move, then the items are marked `InTrash`. If Sift is
//! killed half-way, the next `open` sees the `Pending` items and settles each
//! one by looking at where the file actually is.
//!
//! The history file is never overwritten blindly: a copy of the last good one
//! is kept as `history.json.bak`, and a file that won't parse is set aside
//! (not replaced) before anything is rebuilt from the trash manifests.

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
    /// Planned and journaled; the move may or may not have happened yet.
    /// Only visible mid-cleanup or after a crash, until `Trash::open` settles it.
    Pending,
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
    /// Something the user should hear about (damaged history, interrupted cleanup).
    notice: Option<String>,
    /// History was lost or damaged: batches may be missing and can be rebuilt
    /// from the manifests in the trash folders (`recover_from_manifests`).
    needs_recovery: bool,
    /// The history file exists but can't be read or set aside. Sift won't move
    /// anything it couldn't write down, nor overwrite the file.
    blocked: Option<String>,
}

fn read_history(path: &Path) -> Result<History, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn bak_path(p: &Path) -> PathBuf {
    p.with_extension("json.bak")
}

/// Put a damaged history file somewhere safe instead of overwriting it.
fn set_aside(path: &Path) -> Result<PathBuf, String> {
    let name = format!("history.corrupt-{}.json", now());
    let dest = path.with_file_name(name);
    fs::rename(path, &dest).or_else(|_| fs::copy(path, &dest).map(|_| ())).map_err(|e| e.to_string())?;
    Ok(dest)
}

type Loaded = (History, Option<String>, bool, Option<String>);

/// Load the history. Never throws it away silently.
fn load_history(path: &Path) -> Loaded {
    match fs::read(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            // gone entirely: use the last good copy if there is one
            if let Ok(h) = read_history(&bak_path(path)) {
                return (h, Some("Sift's history file was missing, so the last saved copy was restored. Cleanups made since may not be listed; Sift looked in the trash folders for them.".into()), true, None);
            }
            (History::default(), None, false, None)
        }
        Err(e) => {
            // exists but unreadable (locked, no permission): don't touch it
            let why = format!("Sift can't read its history file ({e}). Nothing will be moved until that's fixed: {}", path.display());
            (History::default(), Some(why.clone()), false, Some(why))
        }
        Ok(bytes) => match serde_json::from_slice::<History>(&bytes) {
            Ok(h) => (h, None, false, None),
            Err(parse_err) => {
                let kept = match set_aside(path) {
                    Ok(k) => k,
                    Err(e) => {
                        let why = format!("Sift's history file is damaged ({parse_err}) and couldn't be set aside ({e}). Nothing will be moved until that's fixed: {}", path.display());
                        return (History::default(), Some(why.clone()), false, Some(why));
                    }
                };
                match read_history(&bak_path(path)) {
                    Ok(h) => (h, Some(format!("Sift's history file was damaged ({parse_err}). The last saved copy was restored and the damaged one kept as {}. Cleanups made since may not be listed; Sift looked in the trash folders for them.", kept.display())), true, None),
                    Err(_) => (History::default(), Some(format!("Sift's history file was damaged ({parse_err}) and has been kept as {}. Sift looked in the trash folders to rebuild the list; anything it couldn't find is still in .sift-trash on that drive.", kept.display())), true, None),
                }
            }
        },
    }
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
        let (history, notice, needs_recovery, blocked) = load_history(&history_path);
        let mut t = Self { history_path, fallback_root, history, notice, needs_recovery, blocked };
        t.settle_pending();
        t
    }

    /// Something worth telling the user, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn dismiss_notice(&mut self) {
        if self.blocked.is_none() {
            self.notice = None;
        }
    }

    /// True when the history was damaged or lost and `recover_from_manifests` should run.
    pub fn needs_recovery(&self) -> bool {
        self.needs_recovery
    }

    /// After a crash: items journaled as `Pending` are `InTrash` if the file
    /// is in its slot, and never happened otherwise.
    fn settle_pending(&mut self) {
        let mut settled = 0usize;
        let mut dropped = 0usize;
        for b in self.history.batches.iter_mut() {
            let BatchKind::Quarantine { entries } = &mut b.kind else { continue };
            if !entries.iter().any(|e| e.state == EntryState::Pending) {
                continue;
            }
            entries.retain_mut(|e| {
                if e.state != EntryState::Pending {
                    return true;
                }
                if fs::symlink_metadata(&e.stored).is_ok() {
                    e.state = EntryState::InTrash;
                    settled += 1;
                    true
                } else {
                    if let Some(slot) = e.stored.parent() {
                        let _ = fs::remove_dir(slot);
                    }
                    dropped += 1;
                    false
                }
            });
        }
        if settled + dropped == 0 {
            return;
        }
        self.history.batches.retain(|b| !matches!(&b.kind, BatchKind::Quarantine { entries } if entries.is_empty()));
        let batches: Vec<Batch> = self.history.batches.clone();
        for b in &batches {
            self.write_manifest(b);
        }
        let _ = self.save();
        let s_plural = if settled == 1 { "" } else { "s" };
        let msg = format!(
            "Sift was closed in the middle of a cleanup. {settled} item{s_plural} that had already moved {} listed in History; {dropped} that hadn't moved {} left in place.",
            if settled == 1 { "is" } else { "are" },
            if dropped == 1 { "was" } else { "were" },
        );
        self.notice = Some(match self.notice.take() {
            Some(n) => format!("{n} {msg}"),
            None => msg,
        });
    }

    /// Rebuild missing batches from the `manifest.json` files in
    /// `<volume>\.sift-trash\<batch>\`. Items whose stored copy is gone are
    /// not brought back. Returns how many batches were added.
    pub fn recover_from_manifests(&mut self, volumes: &[PathBuf]) -> usize {
        if self.blocked.is_some() {
            return 0;
        }
        let mut roots: Vec<PathBuf> = volumes.iter().map(|v| v.join(TRASH_DIR)).collect();
        roots.push(self.fallback_root.clone());
        roots.sort();
        roots.dedup();
        let mut added = 0;
        for root in roots {
            let Ok(rd) = fs::read_dir(&root) else { continue };
            for d in rd.flatten() {
                let Ok(bytes) = fs::read(d.path().join("manifest.json")) else { continue };
                let Ok(mut batch) = serde_json::from_slice::<Batch>(&bytes) else { continue };
                if self.history.batches.iter().any(|b| b.id == batch.id) {
                    continue;
                }
                let BatchKind::Quarantine { entries } = &mut batch.kind else { continue };
                for e in entries.iter_mut() {
                    if e.state == EntryState::Pending || e.state == EntryState::InTrash {
                        e.state = if fs::symlink_metadata(&e.stored).is_ok() {
                            EntryState::InTrash
                        } else if fs::symlink_metadata(&e.original).is_ok() {
                            EntryState::Restored
                        } else {
                            EntryState::Purged
                        };
                    }
                }
                if batch.bytes_in_trash() == 0 {
                    continue;
                }
                self.history.batches.push(batch);
                added += 1;
            }
        }
        if added > 0 {
            self.history.batches.sort_by_key(|b| b.created);
            let _ = self.save();
        }
        self.needs_recovery = false;
        if added > 0 {
            let msg = format!("{added} cleanup{} found in the trash folders and added back to History.", if added == 1 { " was" } else { "s were" });
            self.notice = Some(match self.notice.take() {
                Some(n) => format!("{n} {msg}"),
                None => msg,
            });
        }
        added
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn bytes_in_trash(&self) -> u64 {
        self.history.batches.iter().map(|b| b.bytes_in_trash()).sum()
    }

    /// Write the history atomically (temp file, then rename), keeping the
    /// previous version as `history.json.bak`.
    fn save(&self) -> Result<(), String> {
        if let Some(why) = &self.blocked {
            return Err(why.clone());
        }
        if let Some(dir) = self.history_path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.history_path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(&self.history).map_err(|e| e.to_string())?;
        fs::write(&tmp, json).map_err(|e| format!("couldn't write {}: {e}", tmp.display()))?;
        // only a file that still parses is worth keeping as the backup
        if self.history_path.exists() && read_history(&self.history_path).is_ok() {
            let _ = fs::copy(&self.history_path, bak_path(&self.history_path));
        }
        fs::rename(&tmp, &self.history_path).map_err(|e| format!("couldn't replace {}: {e}", self.history_path.display()))
    }

    /// `save`, reporting a failure in the outcome so the UI can show it.
    fn save_into(&self, out: &mut Outcome) {
        if let Err(e) = self.save() {
            out.skipped.push(Skip { path: self.history_path.clone(), reason: format!("Couldn't update Sift's history file: {e}") });
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
    ///
    /// The batch is journaled (history + manifest, items `Pending`) before the
    /// first file moves, so a crash can't leave files in the trash that
    /// nothing knows about. If the journal can't be written, nothing moves.
    pub fn quarantine(&mut self, label: &str, targets: &[(PathBuf, u64)], contents: bool) -> Outcome {
        let mut out = Outcome::default();
        if let Some(why) = self.blocked.clone() {
            out.skipped = targets.iter().map(|(p, _)| Skip { path: p.clone(), reason: why.clone() }).collect();
            return out;
        }
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

        // plan: check everything and make the slots, but move nothing yet
        let id = new_id();
        let mut plan: Vec<Entry> = Vec::new();
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
            plan.push(Entry { original: src, stored: dest, bytes, state: EntryState::Pending });
        }
        if plan.is_empty() {
            return out;
        }

        // journal
        let batch = Batch { id: id.clone(), created: now(), label: label.into(), kind: BatchKind::Quarantine { entries: plan.clone() } };
        self.write_manifest(&batch);
        self.history.batches.push(batch);
        if let Err(e) = self.save() {
            let b = self.history.batches.pop().expect("just pushed");
            self.discard_batch_dirs(&b);
            let why = format!("Couldn't record the cleanup in Sift's history, so nothing was moved ({e})");
            out.skipped.extend(plan.into_iter().map(|en| Skip { path: en.original, reason: why.clone() }));
            return out;
        }

        // move
        let mut moved = vec![false; plan.len()];
        for (i, en) in plan.iter().enumerate() {
            match fs::rename(&en.original, &en.stored) {
                Ok(()) => {
                    moved[i] = true;
                    out.done += 1;
                    out.bytes += en.bytes;
                    out.moved.push((en.original.clone(), en.bytes));
                }
                Err(e) => {
                    if let Some(slot) = en.stored.parent() {
                        let _ = fs::remove_dir(slot);
                    }
                    out.skipped.push(Skip { path: en.original.clone(), reason: friendly(&e) });
                }
            }
        }

        // settle the journal: moved items become InTrash, the rest are dropped
        if out.done == 0 {
            let b = self.history.batches.pop().expect("just pushed");
            self.discard_batch_dirs(&b);
        } else {
            let b = self.history.batches.last_mut().expect("just pushed");
            if let BatchKind::Quarantine { entries } = &mut b.kind {
                let mut i = 0;
                entries.retain_mut(|e| {
                    let keep = moved[i];
                    i += 1;
                    if keep {
                        e.state = EntryState::InTrash;
                    }
                    keep
                });
            }
            let b = b.clone();
            self.write_manifest(&b);
            out.batch_id = Some(id);
        }
        self.save_into(&mut out);
        out
    }

    /// Remove the (empty) folders and manifest of a batch that never moved anything.
    fn discard_batch_dirs(&self, batch: &Batch) {
        let BatchKind::Quarantine { entries } = &batch.kind else { return };
        let mut batch_dirs: Vec<PathBuf> = Vec::new();
        for e in entries {
            if let Some(slot) = e.stored.parent() {
                let _ = fs::remove_dir(slot);
                if let Some(bd) = slot.parent() {
                    batch_dirs.push(bd.to_path_buf());
                }
            }
        }
        batch_dirs.sort();
        batch_dirs.dedup();
        for bd in batch_dirs {
            let _ = fs::remove_file(bd.join("manifest.json"));
            let _ = fs::remove_dir(&bd);
        }
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
        self.save_into(&mut out);
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
        self.save_into(&mut out);
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
        // The command has already run. If the record can't be saved now, the
        // next successful save writes it.
        let _ = self.save();
        id
    }
}
