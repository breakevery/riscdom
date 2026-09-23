//! v0.9 sandbox F1 — the QEMU download wiring: slot, status, cancel, adoption, audit.
//!
//! The module itself is exercised by `qemu_download.rs`; what is tested here is the
//! **caller** the F1 batch added to `AppState`, which is the toolchain's shape
//! applied to the other resource. Everything runs against a loopback `MockServer`
//! (see `common`), so no network access is involved and no QEMU is ever started.
//!
//! One thing cannot be exercised end to end: `spec_for_current_platform` refuses on
//! every platform by decision (`docs/qemu-distribution.md` §5 — the project guides,
//! it does not fetch), so a *pinned* spec only exists in these tests, where the test
//! builds one. That is exactly what `download_qemu_now` takes as its first argument.

mod common;

use host_core::state::AppState;
use std::path::PathBuf;
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("riscdom-qemu-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn state(tag: &str) -> AppState {
    AppState::in_memory(unique_dir(tag)).expect("state")
}

/// Actions of the audit events whose action starts with `prefix`, oldest first.
fn download_actions(state: &AppState, prefix: &str) -> Vec<String> {
    let mut actions: Vec<String> = state
        .list_events(500, None, Some(prefix.to_string()))
        .expect("events")
        .into_iter()
        .map(|e| e.action)
        .collect();
    actions.sort();
    actions.dedup();
    actions
}

#[test]
fn no_qemu_release_is_pinned_and_the_guidance_is_the_answer() {
    // The decision, stated as a test: there is no spec on any platform, the error
    // names the platform and carries the install hint, and the hint is not empty.
    let err = host_core::qemu_download::spec_for_current_platform()
        .expect_err("no QEMU release is pinned");
    assert_eq!(err.code(), "unpinned_platform");
    let message = err.to_string();
    println!("{message}");
    assert!(message.contains("no QEMU download is pinned"), "{message}");
    let guidance = host_core::qemu_download::install_guidance();
    assert!(!guidance.trim().is_empty(), "the hint must say something");
    assert!(
        message.contains(&guidance),
        "the refusal carries the guidance: {message}"
    );
}

#[test]
fn a_second_qemu_download_start_is_rejected() {
    let state = state("qemu-busy");
    let (bytes, hash) = common::qemu_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::qemu_spec_for(&server, hash, "qemu-archive");

    let _cancel = state.begin_qemu_download(&spec).expect("first start");
    let err = state
        .begin_qemu_download(&spec)
        .expect_err("a second start must be rejected");
    println!("{err}");
    assert!(err.to_string().contains("already in progress"), "{err}");
    assert!(state.qemu_download_status().in_progress);
    assert!(download_actions(&state, "host.qemu.download.")
        .contains(&"host.qemu.download.start".to_string()));

    state.finish_qemu_download();
    assert!(!state.qemu_download_status().in_progress);
}

#[test]
fn cancelling_a_qemu_download_returns_to_idle() {
    let state = state("qemu-cancel");
    let (bytes, hash) = common::qemu_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::qemu_spec_for(&server, hash, "qemu-archive");
    let install_root = unique_dir("qemu-cancel-install");

    let cancel = state.begin_qemu_download(&spec).expect("start");
    state.cancel_qemu_download().expect("cancel");
    let err = state
        .download_qemu_now(&spec, &install_root, cancel, &mut |_| {})
        .expect_err("a cancelled download must fail");
    println!("{err}");
    assert!(err.to_string().contains("cancelled"), "{err}");
    assert!(
        !state.qemu_download_status().in_progress,
        "the download slot must be released"
    );

    let actions = download_actions(&state, "host.qemu.download.");
    println!("actions: {actions:?}");
    assert!(
        actions.contains(&"host.qemu.download.cancelled".to_string()),
        "{actions:?}"
    );
    assert!(!install_root
        .join(host_core::qemu_download::QEMU_VERSION)
        .exists());
}

#[test]
fn a_mock_qemu_download_installs_and_adopts_the_emulator() {
    let state = state("qemu-flow");
    let (bytes, hash) = common::qemu_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::qemu_spec_for(&server, hash, "qemu-archive");
    let install_root = unique_dir("qemu-flow-install");

    let cancel = state.begin_qemu_download(&spec).expect("start");
    let emulator = state
        .download_qemu_now(&spec, &install_root, cancel, &mut |_| {})
        .expect("the mock download must succeed");
    println!("installed: {}", emulator.display());
    assert!(emulator.is_file());
    assert_eq!(server.hits(), 1);

    // The downloaded emulator is now the active one...
    let view = state.probe_qemu();
    assert!(view.found, "{}", view.diagnostics);
    assert_eq!(view.source, "Manual");
    assert_eq!(
        view.path.as_deref(),
        Some(emulator.display().to_string().as_str())
    );

    // ...and it was persisted, so a restart keeps it.
    let settings = std::fs::read_to_string(state.settings_path()).expect("settings.json");
    println!("settings.json: {settings}");
    assert!(settings.contains("qemu_path"), "{settings}");
    assert!(!state.qemu_download_status().in_progress);

    // The audit trail has both ends of the download.
    let actions = download_actions(&state, "host.qemu.download.");
    println!("actions: {actions:?}");
    assert!(
        actions.contains(&"host.qemu.download.start".to_string()),
        "{actions:?}"
    );
    assert!(
        actions.contains(&"host.qemu.download.done".to_string()),
        "{actions:?}"
    );

    // Idempotent: running again must not hit the network.
    let cancel = state.begin_qemu_download(&spec).expect("second start");
    let again = state
        .download_qemu_now(&spec, &install_root, cancel, &mut |_| {})
        .expect("the second run must succeed without downloading");
    assert_eq!(again, emulator);
    assert_eq!(server.hits(), 1, "the second run must not fetch");

    // The last event the status query reports is the terminal one.
    let status = state.qemu_download_status();
    assert!(!status.in_progress);
    let last = serde_json::to_value(status.last_event.expect("a last event")).unwrap();
    assert_eq!(last["state"], "done", "{last}");
}

#[test]
fn a_qemu_that_does_not_run_is_not_adopted() {
    // The fixture archive holds a file that is *not* executable: the download
    // succeeds and the adoption step is what refuses — the same rule as the
    // toolchain's (`set_qemu_path` runs `<path> --version`).
    let state = state("qemu-not-runnable");
    let (bytes, hash) = common::qemu_archive();
    let server = common::MockServer::start(bytes);
    let spec = common::qemu_spec_for(&server, hash, "qemu-archive");
    let install_root = unique_dir("qemu-not-runnable-install");

    let cancel = state.begin_qemu_download(&spec).expect("start");
    let err = state
        .download_qemu_now(&spec, &install_root, cancel, &mut |_| {})
        .expect_err("an emulator that cannot run must not be adopted");
    println!("{err}");
    assert!(!state.qemu_download_status().in_progress);

    // The failure is audited with the reason, and nothing was adopted.
    let events = state
        .list_events(500, None, Some("host.qemu.download.failed".to_string()))
        .expect("events");
    assert_eq!(events.len(), 1, "{events:?}");
    let detail = serde_json::to_string(&events[0].detail).expect("detail");
    println!("detail: {detail}");
    assert!(detail.contains("\"code\":\"not_runnable\""), "{detail}");
    // Nothing was adopted: whatever this machine has on PATH is still what the
    // probe reports, and nothing is `Manual` (which is what adoption sets).
    let view = state.probe_qemu();
    assert_ne!(view.source, "Manual", "{}", view.diagnostics);
    let settings = std::fs::read_to_string(state.settings_path()).unwrap_or_default();
    assert!(!settings.contains("qemu_path"), "{settings}");
}

#[test]
fn the_qemu_directory_belongs_to_the_instance() {
    // The mirror of the toolchain's `multi_instance` assertion: two states in one
    // process keep their QEMU builds apart.
    let data_a = unique_dir("qemu-dir-a");
    let data_b = unique_dir("qemu-dir-b");
    let a = AppState::with_data_dir(unique_dir("qemu-ws-a"), &data_a).expect("state a");
    let b = AppState::with_data_dir(unique_dir("qemu-ws-b"), data_b.clone()).expect("state b");
    assert_eq!(a.qemu_dir(), data_a.join("qemu"));
    assert_eq!(b.qemu_dir(), data_b.join("qemu"));
    assert_ne!(a.qemu_dir(), b.qemu_dir());
}

#[test]
fn the_two_download_slots_are_independent() {
    // Two resources, two slots: a running toolchain download must not block a QEMU
    // one (and the reverse), which is what makes them two *assemblies* rather than
    // one queue.
    let state = state("qemu-slots");
    let (bytes, hash) = common::qemu_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let qemu_spec = common::qemu_spec_for(&server, hash, "qemu-archive");
    let (tbytes, thash) = common::toolchain_archive_with_executable();
    let tserver = common::MockServer::start(tbytes);
    let toolchain_spec = common::spec_for(&tserver, thash, "toolchain-archive");

    let qemu_cancel = state.begin_qemu_download(&qemu_spec).expect("qemu start");
    let _toolchain_cancel = state
        .begin_toolchain_download(&toolchain_spec)
        .expect("a toolchain download is a different slot");
    assert!(state.qemu_download_status().in_progress);
    assert!(state.toolchain_download_status().in_progress);
    assert!(Arc::strong_count(&qemu_cancel) >= 1);

    state.finish_qemu_download();
    assert!(!state.qemu_download_status().in_progress);
    assert!(
        state.toolchain_download_status().in_progress,
        "finishing one must not release the other"
    );
    state.finish_toolchain_download();
}
