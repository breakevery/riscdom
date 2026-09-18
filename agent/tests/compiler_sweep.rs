//! v0.4 batches 5–6 — sweeping the temp directories this project leaves behind.
//!
//! The tests here run **one at a time**: a sweep with a future cutoff removes every
//! matching directory, so two of them in parallel would delete each other's
//! fixtures (and the backdating below would then fail on a missing path).

use agent::tempdirs::{
    sweep_stale_temp_dirs, sweep_stale_temp_dirs_with_age, DATA_DIR_NAME, TEMP_DIR_MAX_AGE,
    TEMP_PREFIX,
};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// Serialises the sweeps in this binary (see the module docs).
static SERIAL: Mutex<()> = Mutex::new(());

fn scratch(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "{}build-{}-{}",
        TEMP_PREFIX,
        std::process::id(),
        name
    ));
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

/// The fallback data directory (`<temp>/riscdom`) must survive any sweep.
fn assert_data_dir_survives() {
    let data_dir = std::env::temp_dir().join(DATA_DIR_NAME);
    let existed = data_dir.exists();
    // Put something in it, so a sweep that deleted it would be visible.
    if !existed {
        std::fs::create_dir_all(&data_dir).expect("create data dir");
        std::fs::write(data_dir.join("settings.json"), b"{}").expect("write settings");
    }
    let swept = sweep_stale_temp_dirs(SystemTime::now() + Duration::from_secs(3600));
    assert!(
        swept > 0 || !existed,
        "the sweep should have had work to do"
    );
    assert!(
        data_dir.exists(),
        "`<temp>/riscdom` is the fallback data directory and must never be swept"
    );
    if !existed {
        let _ = std::fs::remove_dir_all(&data_dir);
    }
}

#[test]
#[cfg(target_os = "windows")]
fn only_directories_older_than_the_threshold_are_swept() {
    let _guard = SERIAL.lock().expect("serial");
    let old = scratch("sweep-old");
    let fresh = scratch("sweep-fresh");
    backdate(&old, TEMP_DIR_MAX_AGE + Duration::from_secs(3600));

    let removed = sweep_stale_temp_dirs_with_age(TEMP_DIR_MAX_AGE);

    assert!(removed >= 1, "the stale directory must be swept");
    assert!(!old.exists(), "an old temp directory must go away");
    assert!(
        fresh.exists(),
        "a fresh temp directory must survive (a build or test may be using it)"
    );

    assert_data_dir_survives();
}

#[test]
#[cfg(not(target_os = "windows"))]
fn only_directories_older_than_the_cutoff_are_swept() {
    let _guard = SERIAL.lock().expect("serial");
    let fresh = scratch("sweep-fresh");
    // A cutoff in the past cannot reach a directory created a moment ago.
    assert_eq!(
        sweep_stale_temp_dirs(SystemTime::now() - TEMP_DIR_MAX_AGE),
        0
    );
    assert!(fresh.exists());

    // A cutoff in the future reaches everything, which is what "old" means here.
    let swept = sweep_stale_temp_dirs(SystemTime::now() + Duration::from_secs(60));
    assert!(swept >= 1, "the sweep must remove what the cutoff covers");
    assert!(!fresh.exists());

    assert_data_dir_survives();
}

#[test]
fn plain_files_are_never_removed() {
    let _guard = SERIAL.lock().expect("serial");
    let file = std::env::temp_dir().join(format!(
        "{}sweep-file-{}.txt",
        TEMP_PREFIX,
        std::process::id()
    ));
    std::fs::write(&file, b"not a directory").expect("write file");

    sweep_stale_temp_dirs(SystemTime::now() + Duration::from_secs(3600));

    assert!(
        file.exists(),
        "the sweep removes directories only, never files: {}",
        file.display()
    );
    let _ = std::fs::remove_file(&file);
}
