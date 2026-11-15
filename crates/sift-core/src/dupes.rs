use crate::model::{DuplicateGroup, FileEntry};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const SAMPLE: u64 = 1024 * 1024;

/// Hash the first, middle and last MiB plus the length. Cheap even for huge
/// files and very reliable for media/archives; use `full_hash` to confirm.
pub fn sampled_hash(path: &Path, size: u64) -> Option<blake3::Hash> {
    let mut f = File::open(path).ok()?;
    let mut h = blake3::Hasher::new();
    h.update(&size.to_le_bytes());
    let mut buf = vec![0u8; SAMPLE as usize];
    for off in [0, size / 2, size.saturating_sub(SAMPLE)] {
        f.seek(SeekFrom::Start(off)).ok()?;
        let n = f.by_ref().take(SAMPLE).read(&mut buf).ok()?;
        h.update(&buf[..n]);
    }
    Some(h.finalize())
}

pub fn full_hash(path: &Path) -> Option<blake3::Hash> {
    let mut h = blake3::Hasher::new();
    h.update_reader(File::open(path).ok()?).ok()?;
    Some(h.finalize())
}

/// Find likely duplicates among `files` (from `ScanResult::big_files`).
/// Within a group the oldest file comes first: the natural one to keep.
pub fn find_duplicates(files: &[FileEntry], min_size: u64) -> Vec<DuplicateGroup> {
    let mut by_size: HashMap<u64, Vec<&FileEntry>> = HashMap::new();
    for f in files.iter().filter(|f| f.size >= min_size) {
        by_size.entry(f.size).or_default().push(f);
    }
    let mut groups: Vec<DuplicateGroup> = by_size
        .into_par_iter()
        .filter(|(_, v)| v.len() > 1)
        .flat_map_iter(|(size, entries)| {
            let mut by_hash: HashMap<blake3::Hash, Vec<&FileEntry>> = HashMap::new();
            for e in entries {
                if let Some(h) = sampled_hash(&e.path, size) {
                    by_hash.entry(h).or_default().push(e);
                }
            }
            by_hash.into_values().filter(|v| v.len() > 1).map(move |mut v| {
                v.sort_by_key(|e| e.modified);
                DuplicateGroup {
                    size,
                    wasted: size * (v.len() as u64 - 1),
                    modified: v.iter().map(|e| e.modified).collect(),
                    paths: v.into_iter().map(|e| e.path.clone()).collect(),
                }
            })
        })
        .collect();
    groups.sort_unstable_by(|a, b| b.wasted.cmp(&a.wasted));
    groups
}

/// Why a copy wasn't confirmed as a true duplicate of the one being kept.
pub type Mismatch = (std::path::PathBuf, &'static str);

/// Compare every byte of `others` against `keep`, right before anything moves
/// (the scan only sampled them). Returns the confirmed copies and the rest
/// with a reason. Errors if the copy to keep itself can't be read.
pub fn verify_copies(keep: &Path, others: &[std::path::PathBuf]) -> Result<(Vec<std::path::PathBuf>, Vec<Mismatch>), String> {
    let size = std::fs::metadata(keep).map_err(|_| format!("Can't read the copy to keep: {}", keep.display()))?.len();
    let want = full_hash(keep).ok_or_else(|| format!("Can't read the copy to keep: {}", keep.display()))?;
    let checked: Vec<(std::path::PathBuf, Result<(), &'static str>)> = others
        .par_iter()
        .filter(|p| p.as_path() != keep)
        .map(|p| {
            let r = match std::fs::metadata(p) {
                Err(_) => Err("No longer exists"),
                Ok(m) if m.len() != size => Err("Size changed since the scan"),
                Ok(_) => match full_hash(p) {
                    Some(h) if h == want => Ok(()),
                    Some(_) => Err("Content differs from the copy you're keeping"),
                    None => Err("Couldn't read it to compare"),
                },
            };
            (p.clone(), r)
        })
        .collect();
    let mut same = Vec::new();
    let mut differ = Vec::new();
    for (p, r) in checked {
        match r {
            Ok(()) => same.push(p),
            Err(why) => differ.push((p, why)),
        }
    }
    Ok((same, differ))
}
