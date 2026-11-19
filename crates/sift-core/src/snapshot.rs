//! The last scan, saved to disk so Sift opens straight to results instead of
//! starting from nothing.

use crate::model::ScanResult;
use crate::suggest::Report;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

/// Bump when the saved shape changes; older files are ignored.
const FORMAT: u32 = 1;

#[derive(Serialize)]
struct SnapshotRef<'a> {
    format: u32,
    scan: &'a ScanResult,
    report: &'a Report,
}

#[derive(Deserialize)]
struct Snapshot {
    format: u32,
    scan: ScanResult,
    report: Report,
}

/// Write atomically (temp file, then rename) so a crash mid-write can't leave
/// a half file behind.
pub fn save(path: &Path, scan: &ScanResult, report: &Report) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut w = BufWriter::new(fs::File::create(&tmp)?);
        serde_json::to_writer(&mut w, &SnapshotRef { format: FORMAT, scan, report })?;
        w.flush()?;
    }
    fs::rename(&tmp, path)
}

/// `None` when there's no file, it's from another format, or it's unreadable.
pub fn load(path: &Path) -> Option<(ScanResult, Report)> {
    let f = fs::File::open(path).ok()?;
    let s: Snapshot = serde_json::from_reader(BufReader::new(f)).ok()?;
    (s.format == FORMAT).then_some((s.scan, s.report))
}
