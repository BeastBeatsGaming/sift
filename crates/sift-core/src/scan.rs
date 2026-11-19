use crate::model::{DirNode, FileEntry, ScanResult, ScanStats};
use crate::rules::{self, Context, REGEN_RULES};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::{Instant, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Files at least this big are kept individually (largest-files list,
    /// duplicates, stale files).
    pub big_file_min: u64,
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { big_file_min: 10 * 1024 * 1024, threads: 0 }
    }
}

/// Live counters the UI can poll while a scan runs.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    pub current: Mutex<String>,
    cancel: AtomicBool,
}

impl Progress {
    pub fn cancel(&self) {
        self.cancel.store(true, Relaxed);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Relaxed)
    }
}

struct Ctx<'a> {
    opts: &'a ScanOptions,
    progress: &'a Progress,
    big: Mutex<Vec<FileEntry>>,
    cats: Mutex<HashMap<&'static str, u64>>,
    special: Mutex<HashMap<&'static str, Vec<FileEntry>>>,
    errors: AtomicU64,
    cloud: AtomicU64,
}

/// Windows: OneDrive "online-only" placeholders don't occupy local space.
#[cfg(windows)]
fn is_cloud_only(m: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_OPEN: u32 = 0x40000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
    m.file_attributes() & (OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS) != 0
}
#[cfg(not(windows))]
fn is_cloud_only(_: &fs::Metadata) -> bool {
    false
}

fn mtime(m: &fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn ext_of(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => name[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Scan `root` and build the folder tree. Blocks until done or cancelled.
pub fn scan(root: &Path, opts: &ScanOptions, progress: &Progress) -> ScanResult {
    let started = Instant::now();
    let ctx = Ctx {
        opts,
        progress,
        big: Mutex::new(Vec::new()),
        cats: Mutex::new(HashMap::new()),
        special: Mutex::new(HashMap::new()),
        errors: AtomicU64::new(0),
        cloud: AtomicU64::new(0),
    };
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(opts.threads)
        .stack_size(8 * 1024 * 1024)
        .build()
        .expect("thread pool");

    let name = root.to_string_lossy().into_owned();
    let tree = pool.install(|| scan_dir(&ctx, root, name, None, false));

    let mut big = ctx.big.into_inner().unwrap();
    big.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    let mut categories: Vec<(String, u64)> =
        ctx.cats.into_inner().unwrap().into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    categories.sort_unstable_by(|a, b| b.1.cmp(&a.1));
    let mut special: Vec<_> = ctx.special.into_inner().unwrap().into_iter().collect();
    special.sort_unstable_by_key(|(id, _)| *id);

    ScanResult {
        root_path: root.to_path_buf(),
        scanned_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
        stats: ScanStats {
            files: progress.files.load(Relaxed),
            dirs: progress.dirs.load(Relaxed),
            errors: ctx.errors.load(Relaxed),
            cloud_skipped: ctx.cloud.load(Relaxed),
            elapsed_ms: started.elapsed().as_millis() as u64,
        },
        root: tree,
        big_files: big,
        categories,
        special,
    }
}

fn scan_dir(ctx: &Ctx, path: &Path, name: String, regen_rule: Option<&'static str>, inside_regen: bool) -> DirNode {
    let mut node = DirNode { name, size: 0, own_bytes: 0, file_count: 0, children: Vec::new(), regen_rule, denied: false };
    if ctx.progress.is_cancelled() {
        return node;
    }
    ctx.progress.dirs.fetch_add(1, Relaxed);
    if let Ok(mut cur) = ctx.progress.current.try_lock() {
        cur.clear();
        cur.push_str(&path.to_string_lossy());
    }

    let rd = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => {
            ctx.errors.fetch_add(1, Relaxed);
            node.denied = true;
            return node;
        }
    };

    let mut subdirs: Vec<(PathBuf, String)> = Vec::new();
    let mut cats: HashMap<&'static str, u64> = HashMap::new();
    let mut bigs = Vec::new();
    let mut specials: Vec<(&'static str, FileEntry)> = Vec::new();
    let mut is_unreal = false;
    let mut is_cargo = false;

    for entry in rd {
        let Ok(entry) = entry else {
            ctx.errors.fetch_add(1, Relaxed);
            continue;
        };
        let Ok(ft) = entry.file_type() else { continue };
        // Rust reports junctions as symlinks on Windows; skipping both avoids
        // double counting and loops.
        if ft.is_symlink() {
            continue;
        }
        let fname = entry.file_name().to_string_lossy().into_owned();
        if ft.is_dir() {
            subdirs.push((entry.path(), fname));
        } else if ft.is_file() {
            let Ok(m) = entry.metadata() else {
                ctx.errors.fetch_add(1, Relaxed);
                continue;
            };
            if is_cloud_only(&m) {
                ctx.cloud.fetch_add(1, Relaxed);
                continue;
            }
            let size = m.len();
            node.own_bytes += size;
            node.file_count += 1;
            let ext = ext_of(&fname);
            is_unreal |= ext == "uproject";
            is_cargo |= fname == "Cargo.toml";
            *cats.entry(rules::category(&ext)).or_default() += size;
            let special = rules::special_for_ext(&ext);
            if size >= ctx.opts.big_file_min || special.is_some() {
                let fe = FileEntry { path: entry.path(), size, modified: mtime(&m) };
                if let Some(id) = special {
                    specials.push((id, fe.clone()));
                }
                if size >= ctx.opts.big_file_min {
                    bigs.push(fe);
                }
            }
        }
    }

    ctx.progress.files.fetch_add(node.file_count, Relaxed);
    ctx.progress.bytes.fetch_add(node.own_bytes, Relaxed);
    if !cats.is_empty() {
        let mut g = ctx.cats.lock().unwrap();
        for (k, v) in cats {
            *g.entry(k).or_default() += v;
        }
    }
    if !bigs.is_empty() {
        ctx.big.lock().unwrap().extend(bigs);
    }
    if !specials.is_empty() {
        let mut g = ctx.special.lock().unwrap();
        for (id, fe) in specials {
            g.entry(id).or_default().push(fe);
        }
    }

    let is_unity = subdirs.iter().any(|(_, n)| n == "Assets") && subdirs.iter().any(|(_, n)| n == "ProjectSettings");
    let mut children: Vec<DirNode> = subdirs
        .into_par_iter()
        .map(|(p, n)| {
            let rule = if inside_regen {
                None
            } else {
                REGEN_RULES
                    .iter()
                    .find(|r| {
                        r.dir_name == n
                            && match r.context {
                                Context::Any => true,
                                Context::Unity => is_unity,
                                Context::Unreal => is_unreal,
                                Context::Cargo => is_cargo,
                            }
                    })
                    .map(|r| r.id)
            };
            scan_dir(ctx, &p, n, rule, inside_regen || rule.is_some())
        })
        .collect();

    children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    node.size = node.own_bytes + children.iter().map(|c| c.size).sum::<u64>();
    node.file_count += children.iter().map(|c| c.file_count).sum::<u64>();
    node.children = children;
    node
}

/// Size of a path outside (or not found in) a scan. Sequential; meant for
/// small known locations.
pub fn path_size(path: &Path) -> Option<u64> {
    let m = fs::symlink_metadata(path).ok()?;
    if !m.is_dir() {
        return Some(if is_cloud_only(&m) { 0 } else { m.len() });
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(rd) = fs::read_dir(&p) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(e.path());
            } else if let Ok(m) = e.metadata() {
                if !is_cloud_only(&m) {
                    total += m.len();
                }
            }
        }
    }
    Some(total)
}
