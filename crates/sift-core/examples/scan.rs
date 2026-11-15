//! Try the core without the UI:  cargo run --release --example scan -- C:\
use sift_core::*;
use std::sync::atomic::Ordering::Relaxed;
use std::{path::PathBuf, sync::Arc, thread, time::Duration};

fn human(b: u64) -> String {
    let mut n = b as f64;
    for u in ["B", "KB", "MB", "GB"] {
        if n < 1024.0 { return format!("{n:.1} {u}"); }
        n /= 1024.0;
    }
    format!("{n:.1} TB")
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let progress = Arc::new(Progress::default());
    let p = progress.clone();
    let ticker = thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        if p.is_cancelled() { break; }
        eprint!("\r{} files · {}        ", p.files.load(Relaxed), human(p.bytes.load(Relaxed)));
    });

    let scan = scan(&root, &ScanOptions::default(), &progress);
    progress.cancel(); // stops the ticker
    ticker.join().ok();
    let s = &scan.stats;
    eprintln!("\rScanned {} in {} files, {} folders, {:.1}s ({} unreadable)",
        human(scan.root.size), s.files, s.dirs, s.elapsed_ms as f64 / 1000.0, s.errors);

    let report = suggest(&scan, &SuggestOptions::default());
    println!("\nSuggestions:");
    for x in &report.suggestions {
        println!("  {:>10}  {:<9} {}", human(x.bytes), format!("{:?}", x.safety), x.title);
    }
    println!("\nTop folders:");
    for c in scan.root.children.iter().take(10) {
        println!("  {:>10}  {}", human(c.size), c.name);
    }
}
