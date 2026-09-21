//! v0.8 technical debt — instance isolation.
//!
//! Two `AppState`s in one process must not share the things the multi-agent
//! runtime needs to keep apart: the data directory (`settings.json`, the
//! sessions DB, the toolchain download directory) and the VM slot. Before this
//! batch the data directory came from a process-wide `OnceLock`, so the first
//! instance's directory silently won for every later one.

use host::state::AppState;
use std::path::PathBuf;
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-multi-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn an_injected_data_dir_is_where_this_instance_writes() {
    let (ws_a, ws_b) = (unique_dir("ws-a"), unique_dir("ws-b"));
    let (data_a, data_b) = (unique_dir("data-a"), unique_dir("data-b"));

    let a = AppState::with_data_dir(&ws_a, &data_a).expect("state a");
    let b = AppState::with_data_dir(&ws_b, &data_b).expect("state b");

    assert_eq!(a.data_dir(), data_a.as_path());
    assert_eq!(b.data_dir(), data_b.as_path());
    assert_ne!(
        a.settings_path(),
        b.settings_path(),
        "each instance owns its settings file"
    );
    assert_eq!(a.toolchain_dir(), data_a.join("toolchain"));
    assert_eq!(b.toolchain_dir(), data_b.join("toolchain"));

    // The sessions DB is opened inside the injected directory, not a shared one.
    assert!(
        data_a.join("sessions.db").is_file(),
        "sessions DB in data_a"
    );
    assert!(
        data_b.join("sessions.db").is_file(),
        "sessions DB in data_b"
    );

    // Each instance writes to its own file, and neither sees the other's value.
    a.set_theme("dark").expect("a theme");
    b.set_theme("light").expect("b theme");
    let a_text = std::fs::read_to_string(a.settings_path()).expect("a settings");
    let b_text = std::fs::read_to_string(b.settings_path()).expect("b settings");
    assert!(a_text.contains("\"dark\""), "{a_text}");
    assert!(b_text.contains("\"light\""), "{b_text}");

    // "Restart": the injected directory is read back, so it is not just a write path.
    let restarted = AppState::with_data_dir(&ws_a, &data_a).expect("restart a");
    assert_eq!(restarted.theme(), "dark");
}

#[test]
fn two_instances_keep_separate_chains_and_vm_slots() {
    let (ws_a, ws_b) = (unique_dir("chain-a"), unique_dir("chain-b"));
    let a = AppState::with_data_dir(&ws_a, unique_dir("chaindata-a")).expect("a");
    let b = AppState::with_data_dir(&ws_b, unique_dir("chaindata-b")).expect("b");

    // Separate audit chains, each in its own workspace.
    assert!(!Arc::ptr_eq(&a.audit, &b.audit), "separate chains");
    assert!(ws_a.join(".riscdom").join("audit.db").is_file());
    assert!(ws_b.join(".riscdom").join("audit.db").is_file());
    assert_eq!(a.audit_status().expect("a status").count, 0);
    assert_eq!(b.audit_status().expect("b status").count, 0);

    // One VM slot per AppState (the dead-end this batch named): the two slots are
    // distinct objects, so a future run on one instance cannot see the other's VM.
    assert!(
        !Arc::ptr_eq(&a.vm_slot, &b.vm_slot),
        "one VM slot per instance"
    );
    assert!(!a.vm_is_running());
    assert!(!b.vm_is_running());
}
