//! Sift core: scan a drive, then turn the result into cleanup suggestions.
//!
//! ```no_run
//! use sift_core::*;
//! let progress = Progress::default();
//! let scan = scan(std::path::Path::new("C:\\"), &ScanOptions::default(), &progress);
//! let report = suggest(&scan, &SuggestOptions::default());
//! ```

pub mod actions;
pub mod dupes;
pub mod model;
pub mod protect;
pub mod rules;
pub mod scan;
pub mod snapshot;
pub mod suggest;
pub mod trash;
pub mod update;

pub use model::*;
pub use scan::{scan, Progress, ScanOptions};
pub use suggest::{suggest, Report, SuggestOptions};
pub use trash::{Batch, BatchKind, Outcome, Trash};

#[cfg(test)]
mod tests;
