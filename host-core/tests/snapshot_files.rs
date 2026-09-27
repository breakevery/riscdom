//! Stage 19c — snapshot listing and deletion (no QEMU needed).

use host_core::state::AppState;

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

#[test]
fn each_instance_gets_its_own_snapshot_directory() {
    // v0.8 batch B made the directory per *agent* so two agents sharing one
    // workspace could not overwrite each other's snapshot names; v1.0 M2a-1 moved
    // it per *instance* (`snapshots/<device>/<instance_id>`), because a node may
    // now run several guests at once. Both older layouts stay readable.
    let workspace = unique_dir("per-instance");
    let root = workspace.join(".riscdom").join("snapshots");
    std::fs::create_dir_all(&root).expect("snapshot root");
    // A snapshot written before the per-agent layout lives in the shared root.
    std::fs::write(root.join("old.mig"), vec![1u8; 16]).expect("old snapshot");

    let a = AppState::in_memory(&workspace).expect("state a");
    let b = AppState::in_memory(&workspace).expect("state b");
    assert_ne!(a.agent_id(), b.agent_id(), "one identity per instance");
    assert_ne!(a.snapshot_dir(), b.snapshot_dir());
    assert_eq!(
        a.snapshot_dir(),
        root.join(agent::device())
            .join(a.current_instance_id().as_str()),
        "the instance's own directory under the device's"
    );

    // The layout v0.8 wrote — one directory per agent — is still read.
    let legacy = root.join(a.agent_id());
    std::fs::create_dir_all(&legacy).expect("legacy dir");
    std::fs::write(legacy.join("per-agent.mig"), vec![9u8; 8]).expect("legacy file");
    assert!(
        a.list_snapshots()
            .expect("list")
            .iter()
            .any(|s| s.name == "per-agent"),
        "the per-agent layout is still listed"
    );

    let listing = |state: &AppState| -> Vec<String> {
        let mut names: Vec<String> = state
            .list_snapshots()
            .expect("list")
            .into_iter()
            .map(|s| s.name)
            .collect();
        names.sort();
        names
    };
    assert_eq!(
        listing(&a),
        vec!["old".to_string(), "per-agent".to_string()],
        "instance {:?}, legacy {:?}, root {:?}",
        a.snapshot_dir(),
        legacy,
        root
    );

    std::fs::create_dir_all(a.snapshot_dir()).expect("a dir");
    std::fs::create_dir_all(b.snapshot_dir()).expect("b dir");
    std::fs::write(a.snapshot_dir().join("same.mig"), vec![2u8; 64]).expect("a same");
    std::fs::write(b.snapshot_dir().join("same.mig"), vec![3u8; 128]).expect("b same");

    // Both agents see the shared history plus their own `same`, and each `same` is
    // its own file, not the other's. `per-agent.mig` was written into **a**'s
    // private legacy directory, so only `a` sees it: the per-agent layout was per
    // agent exactly because that is what it was.
    assert_eq!(
        listing(&a),
        vec![
            "old".to_string(),
            "per-agent".to_string(),
            "same".to_string()
        ]
    );
    assert_eq!(listing(&b), vec!["old".to_string(), "same".to_string()]);
    let a_same = a
        .list_snapshots()
        .expect("list")
        .into_iter()
        .find(|s| s.name == "same")
        .expect("a same");
    assert_eq!(a_same.size_bytes, 64, "a's own copy, not b's");

    // The pre-v0.8 snapshot is still deletable, from the shared root.
    assert!(a.delete_snapshot("old").expect("delete old"));
    assert!(!root.join("old.mig").exists());
    // So is the per-agent one.
    assert!(a.delete_snapshot("per-agent").expect("delete per-agent"));
    assert!(!legacy.join("per-agent.mig").exists());
    assert_eq!(a.list_snapshots().expect("list").len(), 1);
}
