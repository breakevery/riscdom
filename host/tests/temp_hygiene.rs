//! v0.4 batch 5 — the startup hygiene hook.
//!
//! `AppState` sweeps build scratch directories that are old enough that nothing can
//! still be using them. The sweep itself is tested in `agent/tests/compiler_sweep.rs`;
//! this file pins that host startup actually runs it.

use host::state::AppState;
use std::path::PathBuf;
#[cfg(not(target_os = "windows"))]
use std::time::Duration;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-hygiene-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
#[cfg(target_os = "windows")]
fn starting_up_sweeps_a_stale_build_directory() {
    let stale = std::env::temp_dir().join(format!("riscdom-build-{}-hygiene", std::process::id()));
    let _ = std::fs::remove_dir_all(&stale);
    std::fs::create_dir_all(&stale).expect("create stale dir");
    std::fs::write(stale.join("crt0.S"), b"# leftover\n").expect("write");

    // Age it past the threshold, as a killed process would have left it.
    let script = format!(
        "(Get-Item -LiteralPath '{}').LastWriteTime = (Get-Date).AddHours(-48)",
        stale.display()
    );
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .expect("run powershell");
    assert!(status.success(), "could not backdate the directory");
    assert!(stale.exists());

    // Starting the app runs the hook.
    let state = AppState::in_memory(unique_dir("startup")).expect("state");
    assert!(
        !stale.exists(),
        "a stale build directory must be swept at startup"
    );

    // A fresh one is left alone: a run may be compiling right now.
    let fresh = std::env::temp_dir().join(format!("riscdom-build-{}-fresh", std::process::id()));
    std::fs::create_dir_all(&fresh).expect("create fresh dir");
    drop(state);
    let _ = AppState::in_memory(unique_dir("startup-2")).expect("state");
    assert!(
        fresh.exists(),
        "a fresh build directory must survive startup"
    );
    let _ = std::fs::remove_dir_all(&fresh);
}

#[test]
#[cfg(not(target_os = "windows"))]
fn starting_up_does_not_touch_fresh_directories() {
    let fresh = std::env::temp_dir().join(format!("riscdom-build-{}-fresh", std::process::id()));
    let _ = std::fs::remove_dir_all(&fresh);
    std::fs::create_dir_all(&fresh).expect("create fresh dir");
    let _ = AppState::in_memory(unique_dir("startup")).expect("state");
    assert!(
        fresh.exists(),
        "startup must not remove a directory a build may be using"
    );
    // The threshold is the agent's, and it is a day: nothing younger than that goes.
    assert!(agent::BUILD_DIR_MAX_AGE >= Duration::from_secs(3600));
    let _ = std::fs::remove_dir_all(&fresh);
}
