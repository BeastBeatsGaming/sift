//! Keep a finished scan in step with cleanups, so the UI doesn't need a
//! rescan after every action.

use crate::model::{DirNode, ScanResult};
use crate::scan::path_size;
use crate::suggest::Report;
use std::path::{Component, Path, PathBuf};

fn names_eq(a: &str, b: &str) -> bool {
    if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b }
}

fn under(p: &Path, root: &Path) -> bool {
    let (p, r) = (p.to_string_lossy(), root.to_string_lossy());
    let (p, r) = if cfg!(windows) { (p.to_lowercase(), r.to_lowercase()) } else { (p.into_owned(), r.into_owned()) };
    let r = r.trim_end_matches(['\\', '/']);
    p == r || p.starts_with(&format!("{r}\\")) || p.starts_with(&format!("{r}/"))
}

/// Remove `rel` from the tree. Returns (bytes, files) removed.
fn remove_in(node: &mut DirNode, rel: &[String], bytes: u64) -> Option<(u64, u64)> {
    let name = rel.first()?;
    let r = match node.children.iter().position(|c| names_eq(&c.name, name)) {
        Some(i) if rel.len() == 1 => {
            let c = node.children.remove(i);
            (c.size, c.file_count)
        }
        Some(i) => remove_in(&mut node.children[i], &rel[1..], bytes)?,
        // not a folder: a file directly inside this one
        None if rel.len() == 1 => {
            let b = bytes.min(node.own_bytes);
            node.own_bytes -= b;
            (b, 1)
        }
        None => return None,
    };
    node.size = node.size.saturating_sub(r.0);
    node.file_count = node.file_count.saturating_sub(r.1);
    node.children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    Some(r)
}

impl ScanResult {
    /// Drop a path (file or folder) that was removed from disk.
    pub fn forget(&mut self, path: &Path, bytes: u64) {
        if let Ok(rel) = path.strip_prefix(&self.root_path) {
            let rel: Vec<String> = rel
                .components()
                .filter_map(|c| match c {
                    Component::Normal(n) => Some(n.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            remove_in(&mut self.root, &rel, bytes);
        }
        self.big_files.retain(|f| !under(&f.path, path));
        for (_, files) in self.special.iter_mut() {
            files.retain(|f| !under(&f.path, path));
        }
    }
}

impl Report {
    /// Drop removed paths from every suggestion. Items of the suggestions in
    /// `remeasure` are re-sized from disk (their folders stay, contents went).
    pub fn forget(&mut self, removed: &[PathBuf], remeasure: &[&str]) {
        for s in self.suggestions.iter_mut() {
            s.items.retain(|it| !removed.iter().any(|r| under(&it.path, r)));
            if remeasure.contains(&s.id.as_str()) {
                for it in s.items.iter_mut() {
                    it.bytes = path_size(&it.path).unwrap_or(0);
                }
                s.items.retain(|it| it.bytes > 0);
            }
            s.bytes = s.items.iter().map(|i| i.bytes).sum();
        }
        for g in self.duplicates.iter_mut() {
            let keep: Vec<bool> = g.paths.iter().map(|p| !removed.iter().any(|r| under(p, r))).collect();
            let mut i = 0;
            g.modified.retain(|_| (keep.get(i).copied().unwrap_or(true), i += 1).0);
            let mut j = 0;
            g.paths.retain(|_| (keep[j], j += 1).0);
            g.wasted = g.size * (g.paths.len().max(1) as u64 - 1);
        }
        self.duplicates.retain(|g| g.paths.len() > 1);
        if let Some(d) = self.suggestions.iter_mut().find(|s| s.id == "duplicates") {
            d.bytes = self.duplicates.iter().map(|g| g.wasted).sum();
        }
        // duplicates' items are the extra copies: keep consistent with groups
        let dupes = &self.duplicates;
        if let Some(d) = self.suggestions.iter_mut().find(|s| s.id == "duplicates") {
            d.items.retain(|it| dupes.iter().any(|g| g.paths.iter().skip(1).any(|p| p == &it.path)));
        }
        self.suggestions.retain(|s| s.bytes > 0);
        self.suggestions.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes));
    }
}
