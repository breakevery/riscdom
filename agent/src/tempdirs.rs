//! Temporary scratch directories (v0.4 batches 5–6).
//!
//! Everything this project drops in the system temp directory is named
//! `riscdom-*`: per-build scratch (see [`crate::compiler`]), test workspaces, demo
//! output. None of it is cleaned by the thing that created it — a build cleans up
//! after itself, but a killed process cannot — so the host sweeps the old ones at
//! startup.
//!
//! Two hard rules keep the sweep safe:
//!
//! - **`<temp>/riscdom` is never a target.** It is not scratch: it is the fallback
//!   data directory (settings, sessions, downloaded toolchains) used when the OS
//!   app-data directory is unavailable.
//! - **Only directories are removed, and only by that prefix.** A file named
//!   `riscdom-…` is left alone, and nothing outside the prefix is ever touched.

use std::time::{Duration, SystemTime};

/// Prefix of every temporary entry this project creates.
pub const TEMP_PREFIX: &str = "riscdom-";

/// The fallback data directory's name (`<temp>/riscdom`) — never a sweep target.
pub const DATA_DIR_NAME: &str = "riscdom";

/// How old a temporary directory must be before a sweep may remove it.
///
/// A day is long enough that no running build, test or example can still own it.
pub const TEMP_DIR_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Remove `riscdom-*` **directories** whose last write is older than `cutoff`.
///
/// The protective rules are in the module docs. Returns how many directories went
/// away.
pub fn sweep_stale_temp_dirs(cutoff: SystemTime) -> usize {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(TEMP_PREFIX) || name == DATA_DIR_NAME {
            continue;
        }
        // `file_type` does not follow symlinks: only real directories qualify.
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {}
            _ => continue,
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if modified >= cutoff {
            continue;
        }
        if std::fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// [`sweep_stale_temp_dirs`] with a relative age.
pub fn sweep_stale_temp_dirs_with_age(max_age: Duration) -> usize {
    sweep_stale_temp_dirs(SystemTime::now() - max_age)
}
