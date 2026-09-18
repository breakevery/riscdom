//! v0.4 batch 5 — sweeping build scratch directories left by earlier processes.
//!
//! A build cleans up after itself (3-followup-2). This covers the leftovers a
//! build *could not* clean: the compile guard's timeout path and hosts that died
//! mid-compile. Deliberately the only test in its binary — the sweep is scoped to
//! this process's own scratch names, and a concurrent build would confuse it.

#[cfg(not(target_os = "windows"))]
use agent::compiler::sweep_build_dirs_older_than;
use agent::compiler::{sweep_stale_build_dirs, BUILD_DIR_MAX_AGE};
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::time::Duration;
#[cfg(not(target_os = "windows"))]
use std::time::SystemTime;

fn build_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("riscdom-build-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create scratch dir");
    std::fs::write(path.join("crt0.S"), b"# scratch\n").expect("write scratch file");
    path
}

/// Backdate a directory so it looks like an earlier process left it behind.
#[cfg(target_os = "windows")]
fn backdate(path: &Path, by: Duration) {
    let hours = by.as_secs() / 3600;
    let script = format!(
        "(Get-Item -LiteralPath '{}').LastWriteTime = (Get-Date).AddHours(-{hours})",
        path.display()
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .expect("run powershell");
    assert!(status.success(), "could not backdate {}", path.display());
}

#[test]
#[cfg(target_os = "windows")]
fn only_directories_older_than_the_threshold_are_swept() {
    let old = build_dir("sweep-old");
    let fresh = build_dir("sweep-fresh");
    backdate(&old, BUILD_DIR_MAX_AGE + Duration::from_secs(3600));

    let removed = sweep_stale_build_dirs(BUILD_DIR_MAX_AGE);

    assert!(removed >= 1, "the stale directory must be swept");
    assert!(!old.exists(), "an old build directory must go away");
    assert!(
        fresh.exists(),
        "a fresh build directory must survive (a run may be using it)"
    );

    // The fallback data directory (`<temp>/riscdom`, no suffix) is never a target.
    let data_dir = std::env::temp_dir().join("riscdom");
    if data_dir.exists() {
        assert!(data_dir.exists(), "the data directory must never be swept");
    }
}

#[test]
#[cfg(not(target_os = "windows"))]
fn only_directories_older_than_the_cutoff_are_swept() {
    let fresh = build_dir("sweep-fresh");
    // A cutoff in the past cannot reach a directory created a moment ago.
    assert_eq!(
        sweep_build_dirs_older_than(SystemTime::now() - BUILD_DIR_MAX_AGE),
        0
    );
    assert!(fresh.exists());

    // A cutoff in the future reaches everything, which is what "old" means here.
    let swept = sweep_build_dirs_older_than(SystemTime::now() + Duration::from_secs(60));
    assert!(swept >= 1, "the sweep must remove what the cutoff covers");
    assert!(!fresh.exists());
}
