//! Stage 19c — snapshot listing and deletion (no QEMU needed).

use host::state::AppState;

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-snapfiles-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn lists_and_deletes_both_snapshot_modes() {
    let workspace = unique_dir("list");
    let dir = workspace.join(".riscdom").join("snapshots");
    std::fs::create_dir_all(&dir).expect("snapshot dir");
    std::fs::write(dir.join("real.mig"), vec![7u8; 2048]).expect("write mig");
    std::fs::write(dir.join("legacy.json"), b"{}").expect("write json");
    std::fs::write(dir.join("ignore.txt"), b"x").expect("write other");

    let state = AppState::in_memory(&workspace).expect("state");

    let mut snapshots = state.list_snapshots().expect("list");
    snapshots.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(snapshots.len(), 2, "only .mig/.json count: {snapshots:?}");

    let real = snapshots.iter().find(|s| s.name == "real").expect("real");
    assert_eq!(real.mode, "tcp-relay");
    assert_eq!(real.size_bytes, 2048);
    println!(
        "real snapshot: {} ({} bytes, {})",
        real.name, real.size_bytes, real.mode
    );

    let legacy = snapshots
        .iter()
        .find(|s| s.name == "legacy")
        .expect("legacy");
    assert_eq!(legacy.mode, "reboot-fallback");

    // Deleting removes the file; unknown names report `false`.
    assert!(state.delete_snapshot("real").expect("delete"));
    assert!(!state.delete_snapshot("nope").expect("delete missing"));
    assert_eq!(state.list_snapshots().expect("list").len(), 1);
    assert!(!dir.join("real.mig").exists());
}

#[test]
fn missing_snapshot_dir_lists_empty() {
    let state = AppState::in_memory(unique_dir("empty")).expect("state");
    assert!(state.list_snapshots().expect("list").is_empty());
}
