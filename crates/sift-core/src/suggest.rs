use crate::dupes::find_duplicates;
use crate::model::*;
use crate::protect;
use crate::rules::{self, KnownAction, KNOWN_LOCATIONS, SPECIAL_EXTS};
use crate::scan::path_size;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MB: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SuggestOptions {
    /// Hide suggestions smaller than this.
    pub min_bytes: u64,
    pub regen_min: u64,
    pub dup_min: u64,
    pub stale_min: u64,
    pub stale_days: i64,
    pub downloads_days: i64,
    pub find_duplicates: bool,
    /// Paths never suggested, including everything inside them.
    pub ignored: Vec<PathBuf>,
    /// Extra folders treated like Program Files (never suggested). The real
    /// Program Files / ProgramData / AppData and game libraries are always
    /// guarded; this adds to them.
    pub guarded_dirs: Vec<PathBuf>,
    /// Extra cloud-synced roots, added to the OneDrive folders found on the
    /// machine. Files in them are never suggested for archiving.
    pub synced_dirs: Vec<PathBuf>,
}

impl Default for SuggestOptions {
    fn default() -> Self {
        Self {
            min_bytes: 50 * MB,
            regen_min: 20 * MB,
            dup_min: 50 * MB,
            stale_min: 300 * MB,
            stale_days: 365,
            downloads_days: 60,
            find_duplicates: true,
            ignored: Vec::new(),
            guarded_dirs: Vec::new(),
            synced_dirs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub suggestions: Vec<Suggestion>,
    pub duplicates: Vec<DuplicateGroup>,
}

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// What scan-derived suggestions must stay away from: installed software and
/// app data (moving it breaks things) and cloud-synced folders (moving a file
/// out deletes it from the cloud too).
pub struct Guards {
    software: Vec<(PathBuf, protect::Guarded)>,
    synced: Vec<PathBuf>,
}

impl Guards {
    pub fn new(opts: &SuggestOptions) -> Self {
        let mut software = protect::software_roots();
        software.extend(opts.guarded_dirs.iter().map(|p| (p.clone(), protect::Guarded::ProgramFiles)));
        let mut synced = protect::onedrive_roots();
        synced.extend(opts.synced_dirs.iter().cloned());
        Self { software, synced }
    }

    /// Why `p` can't be part of a scan-derived suggestion, if it can't.
    pub fn software(&self, p: &Path) -> Option<&'static str> {
        protect::guarded_in(p, &self.software).map(|g| g.reason())
    }

    pub fn synced(&self, p: &Path) -> bool {
        protect::in_synced(p, &self.synced)
    }
}

/// Suggestions found by looking through the scan (as opposed to the named
/// cache locations in `rules.rs`). Only these are subject to `Guards`.
pub fn is_scan_derived(id: &str) -> bool {
    matches!(id, "stale" | "duplicates" | "downloads") || id.starts_with("regen:") || id.starts_with("ext:")
}

/// Of those, the ones meant to archive things you own rather than clear junk:
/// removing a synced file here deletes it from the cloud as well.
pub fn is_archive_like(id: &str) -> bool {
    matches!(id, "stale" | "duplicates" | "downloads")
}

pub fn suggest(scan: &ScanResult, opts: &SuggestOptions) -> Report {
    let mut out = Vec::new();
    let guards = Guards::new(opts);

    // 1. Known cache / junk locations
    for loc in KNOWN_LOCATIONS {
        let mut items: Vec<Located> = Vec::new();
        for pat in loc.paths {
            for p in expand(pat) {
                let size = tree_size(scan, &p).or_else(|| path_size(&p));
                if let Some(bytes) = size.filter(|b| *b > 0) {
                    items.push(Located { path: p, bytes });
                }
            }
        }
        let bytes: u64 = items.iter().map(|i| i.bytes).sum();
        if bytes < opts.min_bytes {
            continue;
        }
        items.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
        let action = match loc.action {
            KnownAction::Quarantine => Action::Quarantine { contents: true },
            KnownAction::Command(p, a) => Action::Command { program: p.into(), args: a.iter().map(|s| s.to_string()).collect() },
            KnownAction::Settings(u) => Action::OpenSettings { uri: u.into() },
            KnownAction::Manual => Action::Manual,
        };
        out.push(Suggestion {
            id: loc.id.into(),
            title: loc.label.into(),
            bytes,
            safety: loc.safety,
            how: loc.how.into(),
            permanent: is_permanent(loc.id, &action),
            action,
            items,
        });
    }

    // 2. Regenerable dev folders, one suggestion per rule
    let mut regen: HashMap<&'static str, Vec<Located>> = HashMap::new();
    for ch in &scan.root.children {
        collect_regen(ch, scan.root_path.join(&ch.name), opts.regen_min, &guards, &mut regen);
    }
    for (id, mut items) in regen {
        let rule = rules::regen_rule(id).expect("rule");
        let bytes: u64 = items.iter().map(|i| i.bytes).sum();
        if bytes < opts.min_bytes {
            continue;
        }
        items.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
        out.push(Suggestion {
            id: format!("regen:{id}"),
            title: format!("{} — {} folder{}", rule.label, items.len(), if items.len() == 1 { "" } else { "s" }),
            bytes,
            safety: rule.safety,
            how: rule.how.into(),
            action: Action::Quarantine { contents: false },
            permanent: false,
            items,
        });
    }

    // 3. Special extensions (Blender backups, dumps)
    for (id, files) in &scan.special {
        let rule = SPECIAL_EXTS.iter().find(|s| s.id == *id).expect("rule");
        let bytes: u64 = files.iter().map(|f| f.size).sum();
        if bytes < opts.min_bytes {
            continue;
        }
        let mut items: Vec<Located> = files.iter().filter(|f| guards.software(&f.path).is_none()).map(|f| Located { path: f.path.clone(), bytes: f.size }).collect();
        let bytes: u64 = items.iter().map(|i| i.bytes).sum();
        if bytes < opts.min_bytes {
            continue;
        }
        items.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
        out.push(Suggestion {
            id: format!("ext:{id}"),
            title: format!("{} — {} file{}", rule.label, items.len(), if items.len() == 1 { "" } else { "s" }),
            bytes,
            safety: rule.safety,
            how: rule.how.into(),
            action: Action::Quarantine { contents: false },
            permanent: false,
            items,
        });
    }

    // 4. Large files untouched for a long time
    let cutoff = now() - opts.stale_days * 86_400;
    let stale: Vec<Located> = scan
        .big_files
        .iter()
        .filter(|f| f.size >= opts.stale_min && f.modified > 0 && f.modified < cutoff)
        .filter(|f| !is_system_file(&f.path))
        .filter(|f| guards.software(&f.path).is_none() && !guards.synced(&f.path))
        .map(|f| Located { path: f.path.clone(), bytes: f.size })
        .collect();
    let bytes: u64 = stale.iter().map(|i| i.bytes).sum();
    if bytes >= opts.min_bytes {
        out.push(Suggestion {
            id: "stale".into(),
            title: format!("Large files untouched for {}+ days — {} file{}", opts.stale_days, stale.len(), if stale.len() == 1 { "" } else { "s" }),
            bytes,
            safety: Safety::Review,
            how: "Archive to another drive or your NAS if you still want them.".into(),
            action: Action::Quarantine { contents: false },
            permanent: false,
            items: stale,
        });
    }

    // 5. Old Downloads
    if let Some(mut items) = old_downloads(opts.downloads_days) {
        items.retain(|i| guards.software(&i.path).is_none() && !guards.synced(&i.path));
        let bytes: u64 = items.iter().map(|i| i.bytes).sum();
        if bytes >= opts.min_bytes {
            out.push(Suggestion {
                id: "downloads".into(),
                title: format!("Downloads older than {} days — {} files", opts.downloads_days, items.len()),
                bytes,
                safety: Safety::Review,
                how: "Old installers and archives are usually safe once installed or extracted.".into(),
                action: Action::Quarantine { contents: false },
                permanent: false,
                items,
            });
        }
    }

    // 6. Duplicates
    // Files in installed software or in OneDrive never take part: the copy
    // that gets moved is the one that would break something or vanish from the cloud.
    let duplicates = if opts.find_duplicates {
        let eligible: Vec<FileEntry> = scan.big_files.iter().filter(|f| guards.software(&f.path).is_none() && !guards.synced(&f.path)).cloned().collect();
        find_duplicates(&eligible, opts.dup_min)
    } else {
        Vec::new()
    };
    let wasted: u64 = duplicates.iter().map(|g| g.wasted).sum();
    if wasted >= opts.min_bytes {
        let items = duplicates
            .iter()
            .flat_map(|g| g.paths.iter().skip(1).map(move |p| Located { path: p.clone(), bytes: g.size }))
            .collect();
        out.push(Suggestion {
            id: "duplicates".into(),
            title: format!("Likely duplicate files — {} group{}", duplicates.len(), if duplicates.len() == 1 { "" } else { "s" }),
            bytes: wasted,
            safety: Safety::Review,
            how: "Same size and matching content samples. The oldest copy of each is kept; extras are listed.".into(),
            action: Action::Quarantine { contents: false },
            permanent: false,
            items,
        });
    }

    out.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
    let mut report = Report { suggestions: out, duplicates };
    if !opts.ignored.is_empty() {
        report.forget(&opts.ignored, &[]);
    }
    report
}

/// Sift's own moves go to .sift-trash and can be undone; anything a command
/// or Windows does can't. Manual steps are flagged when following them
/// deletes for good.
pub fn is_permanent(id: &str, action: &Action) -> bool {
    match action {
        Action::Quarantine { .. } => false,
        Action::Command { .. } | Action::OpenSettings { .. } => true,
        Action::Manual => matches!(id, "recycle_bin" | "trash"),
    }
}

fn collect_regen(node: &DirNode, path: PathBuf, min: u64, guards: &Guards, out: &mut HashMap<&'static str, Vec<Located>>) {
    if node.size < min || guards.software(&path).is_some() {
        return;
    }
    if let Some(id) = node.regen_rule {
        out.entry(id).or_default().push(Located { path, bytes: node.size });
        return;
    }
    for ch in &node.children {
        collect_regen(ch, path.join(&ch.name), min, guards, out);
    }
}

fn is_system_file(p: &Path) -> bool {
    let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    matches!(n.as_str(), "pagefile.sys" | "hiberfil.sys" | "swapfile.sys")
}

fn names_eq(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

/// Look a folder up in the scanned tree instead of re-reading the disk.
pub fn tree_size(scan: &ScanResult, path: &Path) -> Option<u64> {
    let rel = path.strip_prefix(&scan.root_path).ok()?;
    let mut node = &scan.root;
    for c in rel.components() {
        let Component::Normal(name) = c else { return None };
        let name = name.to_string_lossy();
        node = node.children.iter().find(|ch| names_eq(&ch.name, &name))?;
    }
    Some(node.size)
}

/// Expand `%VAR%` references and whole-component `*` globs.
pub fn expand(pattern: &str) -> Vec<PathBuf> {
    let mut s = String::new();
    let mut rest = pattern;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else { break };
        let var = &rest[start + 1..start + 1 + len];
        match std::env::var(var) {
            Ok(v) if !v.is_empty() => {
                s.push_str(&rest[..start]);
                s.push_str(&v);
            }
            _ => return Vec::new(),
        }
        rest = &rest[start + len + 2..];
    }
    s.push_str(rest);

    let mut acc = vec![PathBuf::new()];
    for comp in Path::new(&s).components() {
        if comp.as_os_str() == "*" {
            acc = acc
                .into_iter()
                .flat_map(|p| {
                    std::fs::read_dir(&p)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|e| e.file_type().map(|t| t.is_dir() && !t.is_symlink()).unwrap_or(false))
                        .map(|e| e.path())
                        .collect::<Vec<_>>()
                })
                .collect();
        } else {
            for p in acc.iter_mut() {
                p.push(comp.as_os_str());
            }
        }
    }
    acc.retain(|p| p.exists());
    acc
}

fn old_downloads(days: i64) -> Option<Vec<Located>> {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok()?;
    let dir = Path::new(&home).join("Downloads");
    let cutoff = now() - days * 86_400;
    let mut items = Vec::new();
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if !m.is_file() {
            continue;
        }
        let modified = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
        if modified < cutoff {
            items.push(Located { path: e.path(), bytes: m.len() });
        }
    }
    items.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
    Some(items)
}
