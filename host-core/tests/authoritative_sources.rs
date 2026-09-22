//! v0.4 1e-followup — the host reads sandbox / agent values from their owners.
//!
//! These tests are the behavioural half of the mirror guard
//! (`scripts/check-mirrored-constants.mjs`): the host must recognise files and
//! executables by the names the owning crate defines, so a change there reaches
//! the host instead of drifting away from a private copy.

use host_core::state::AppState;
use std::path::{Path, PathBuf};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-sources-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn snapshot_files_named_by_the_sandbox_extensions_are_listed_and_deleted() {
    let dir = unique_dir("snapshot-ext");
    let state = AppState::in_memory(dir.clone()).expect("state");
    let snapshots = dir.join(".riscdom").join("snapshots");
    std::fs::create_dir_all(&snapshots).expect("snapshot dir");

    // Written with the sandbox's own constants, as the sandbox would write them.
    let real = snapshots.join(format!("real.{}", sandbox::SNAPSHOT_MIG_EXT));
    let old = snapshots.join(format!("old.{}", sandbox::SNAPSHOT_JSON_EXT));
    std::fs::write(&real, b"migration stream").expect("write real");
    std::fs::write(&old, b"{}").expect("write fallback");

    let listed = state.list_snapshots().expect("list");
    let names: Vec<(&str, &str)> = listed
        .iter()
        .map(|s| (s.name.as_str(), s.mode.as_str()))
        .collect();
    assert!(names.contains(&("real", "tcp-relay")), "listed: {names:?}");
    assert!(
        names.contains(&("old", "reboot-fallback")),
        "listed: {names:?}"
    );

    // The host finds them again when deleting (same names, from the same owner).
    assert!(state.delete_snapshot("real").expect("delete"));
    assert!(!real.exists(), "the real snapshot must be gone");
    assert!(state.delete_snapshot("old").expect("delete fallback"));
    assert!(!old.exists());
}

#[test]
fn the_archive_predicate_accepts_the_agents_compiler_names() {
    for name in agent::GCC_NAMES {
        assert!(
            host_core::toolchain_download::is_compiler_name(Path::new(name)),
            "{name} must be recognised"
        );
        let with_exe = format!("{name}.exe");
        assert!(
            host_core::toolchain_download::is_compiler_name(Path::new(&with_exe)),
            "{with_exe} must be recognised"
        );
    }
    assert!(!host_core::toolchain_download::is_compiler_name(Path::new(
        "cmd.exe"
    )));
    assert!(!host_core::toolchain_download::is_compiler_name(Path::new(
        "riscv64-unknown-elf-ld.exe"
    )));
}
