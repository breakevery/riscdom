//! Stage 24c — toolchain probe / set / clear through the host state.

mod common;

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice};
use host_core::state::AppState;
use std::path::PathBuf;
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-toolchain-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn state(tag: &str) -> AppState {
    AppState::in_memory(unique_dir(tag)).expect("state")
}

fn final_response(text: &str) -> ChatResponse {
    ChatResponse {
        id: None,
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }
}

#[test]
#[ignore = "requires a discoverable RISC-V GCC; run with --include-ignored"]
fn probe_reports_a_discovered_toolchain() {
    let state = state("probe");
    let view = state.probe_toolchain();
    println!(
        "found={} source={} path={:?}",
        view.found, view.source, view.path
    );

    // These host tests already require a working toolchain (they compile C).
    assert!(view.found, "expected a toolchain: {}", view.diagnostics);
    assert!(
        ["EnvVar", "KnownPath", "Path"].contains(&view.source.as_str()),
        "unexpected source {}",
        view.source
    );
    assert!(view.path.is_some());
    assert!(
        view.diagnostics.contains("RISCDOM_RISCV_GCC"),
        "{}",
        view.diagnostics
    );
}

#[test]
fn a_missing_path_is_rejected() {
    let state = state("missing");
    let err = state
        .set_toolchain_path(r"C:\definitely\not\here\riscv64-unknown-elf-gcc.exe")
        .expect_err("must be rejected");
    assert!(err.to_string().contains("not a file"), "{err}");
}

#[test]
fn a_non_runnable_file_is_rejected() {
    let state = state("notrunnable");
    let bogus = std::env::temp_dir().join("riscdom-not-a-compiler.txt");
    std::fs::write(&bogus, b"not an executable").expect("write");
    let err = state
        .set_toolchain_path(&bogus.display().to_string())
        .expect_err("must be rejected");
    println!("{err}");
    assert!(
        err.to_string().contains("--version") || err.to_string().contains("not runnable"),
        "{err}"
    );
}

#[test]
#[ignore = "requires a discoverable RISC-V GCC; run with --include-ignored"]
fn a_path_as_the_file_picker_returns_it_is_accepted() {
    // A native picker hands back a plain path string; on some platforms that is
    // forward-slash form even on Windows. The command must take it as it comes
    // (v0.4 batch 2).
    let state = state("dialog-path");
    let discovered = state.probe_toolchain().path.expect("discovered path");
    let picked = discovered.replace('\\', "/");

    state
        .set_toolchain_path(&picked)
        .expect("a forward-slash path must be accepted");
    let view = state.probe_toolchain();
    assert!(view.found, "{view:?}");
    assert_eq!(view.source, "Manual");
}

#[test]
#[ignore = "requires a discoverable RISC-V GCC; run with --include-ignored"]
fn manual_path_wins_and_clearing_restores_discovery() {
    let state = state("manual");
    let discovered = state.probe_toolchain().path.expect("discovered path");

    state
        .set_toolchain_path(&discovered)
        .expect("set manual path");
    let view = state.probe_toolchain();
    assert!(view.found);
    assert_eq!(view.source, "Manual");
    assert_eq!(view.path.as_deref(), Some(discovered.as_str()));
    println!("diagnostics (manual):\n{}", view.diagnostics);

    state.clear_toolchain_path().expect("clear");
    let view = state.probe_toolchain();
    assert_ne!(view.source, "Manual", "must fall back to discovery");
    assert!(view.found);
}

#[test]
fn invalid_manual_path_diagnostics_are_not_duplicated() {
    let state = state("duptext");
    let bogus = std::env::temp_dir().join("riscdom-not-a-compiler-2.txt");
    std::fs::write(&bogus, b"nope").expect("write");

    // Diagnosis path (probe): exactly one `not runnable:` prefix.
    *state.toolchain_path.lock().unwrap() = Some(bogus.clone());
    let view = state.probe_toolchain();
    assert!(!view.found);
    assert!(
        !view.diagnostics.contains("not runnable: not runnable:"),
        "{}",
        view.diagnostics
    );
    assert_eq!(
        view.diagnostics.matches("not runnable:").count(),
        1,
        "{}",
        view.diagnostics
    );

    // Setter path: same guarantee.
    *state.toolchain_path.lock().unwrap() = None;
    let err = state
        .set_toolchain_path(&bogus.display().to_string())
        .expect_err("must fail");
    assert!(
        !err.to_string().contains("not runnable: not runnable:"),
        "{err}"
    );
}

#[test]
fn a_second_download_start_is_rejected() {
    let state = state("dl-busy");
    let (bytes, hash) = common::toolchain_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::spec_for(&server, hash, "toolchain-archive");

    let _cancel = state.begin_toolchain_download(&spec).expect("first start");
    let err = state
        .begin_toolchain_download(&spec)
        .expect_err("a second start must be rejected");
    println!("{err}");
    assert!(err.to_string().contains("already in progress"), "{err}");
    assert!(state.toolchain_download_status().in_progress);

    state.finish_toolchain_download();
    assert!(!state.toolchain_download_status().in_progress);
}

#[test]
fn cancelling_a_download_returns_to_idle() {
    let state = state("dl-cancel");
    let (bytes, hash) = common::toolchain_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::spec_for(&server, hash, "toolchain-archive");
    let install_root = unique_dir("dl-cancel-install");

    let cancel = state.begin_toolchain_download(&spec).expect("start");
    state.cancel_toolchain_download().expect("cancel");
    let err = state
        .download_toolchain_now(&spec, &install_root, cancel, &mut |_| {})
        .expect_err("a cancelled download must fail");
    println!("{err}");
    assert!(err.to_string().contains("cancelled"), "{err}");
    assert!(
        !state.toolchain_download_status().in_progress,
        "the download slot must be released"
    );

    let actions = download_actions(&state, "host.toolchain.download.");
    println!("actions: {actions:?}");
    assert!(
        actions.contains(&"host.toolchain.download.cancelled".to_string()),
        "{actions:?}"
    );
    assert!(!install_root
        .join(host_core::toolchain_download::XPACK_RISCV_GCC_VERSION)
        .exists());
}

#[test]
fn a_mock_download_installs_and_adopts_the_toolchain() {
    let state = state("dl-flow");
    let (bytes, hash) = common::toolchain_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::spec_for(&server, hash, "toolchain-archive");
    let install_root = unique_dir("dl-flow-install");

    let cancel = state.begin_toolchain_download(&spec).expect("start");
    let compiler = state
        .download_toolchain_now(&spec, &install_root, cancel, &mut |_| {})
        .expect("the mock download must succeed");
    println!("installed: {}", compiler.display());
    assert!(compiler.is_file());
    assert_eq!(server.hits(), 1);

    // The downloaded compiler is now the active toolchain...
    let view = state.probe_toolchain();
    assert!(view.found, "{}", view.diagnostics);
    assert_eq!(view.source, "Manual");
    assert_eq!(
        view.path.as_deref(),
        Some(compiler.display().to_string().as_str())
    );

    // ... and it was persisted, so a restart keeps it.
    let settings = std::fs::read_to_string(state.settings_path()).expect("settings.json");
    println!("settings.json: {settings}");
    assert!(settings.contains("toolchain_path"), "{settings}");
    assert!(!state.toolchain_download_status().in_progress);

    // Idempotent: running again must not hit the network.
    let cancel = state.begin_toolchain_download(&spec).expect("second start");
    let again = state
        .download_toolchain_now(&spec, &install_root, cancel, &mut |_| {})
        .expect("second run");
    assert_eq!(again, compiler);
    assert_eq!(server.hits(), 1, "the second run must be a no-op");
}

#[test]
fn a_mock_zig_download_installs_and_adopts_the_compiler() {
    let state = state("dl-zig");
    let (bytes, hash) = common::zig_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::zig_spec_for(&server, hash, "zig-archive");
    let install_root = common::unique_dir("dl-zig-install");

    let cancel = state.begin_toolchain_download(&spec).expect("start");
    assert_eq!(
        state.toolchain_download_status().toolchain,
        Some(host_core::toolchain_download::Toolchain::Zig),
        "the status names the toolchain that is running"
    );
    let zig = state
        .download_toolchain_now(&spec, &install_root, cancel, &mut |_| {})
        .expect("the mock download must succeed");
    println!("installed: {}", zig.display());
    assert!(zig.is_file());
    assert_eq!(server.hits(), 1);

    // The downloaded binary is the active Zig compiler now -- and the C pin is untouched,
    // which is the whole point of two single values instead of one map.
    assert_eq!(state.zig_config().zig, zig);
    let settings = std::fs::read_to_string(state.settings_path()).expect("settings.json");
    println!("settings.json: {settings}");
    let parsed: serde_json::Value = serde_json::from_str(&settings).expect("json");
    assert!(parsed["zig_path"].is_string(), "{settings}");
    assert!(
        parsed["toolchain_path"].is_null(),
        "a Zig download must not touch the C pin: {settings}"
    );
    assert!(!state.toolchain_download_status().in_progress);
    assert_eq!(state.toolchain_download_status().toolchain, None);
}

#[test]
fn a_mock_rust_download_installs_and_adopts_the_sysroot() {
    let state = state("dl-rust");
    // The pin has to be this machine's own release: the host refuses a Rust download whose pin
    // disagrees with the `rustc` that would have to use it (decision §52).
    let Some(release) = state.rust_release() else {
        eprintln!(
            "skip: a_mock_rust_download_installs_and_adopts_the_sysroot -- no rustc (ENVIRONMENT.md)"
        );
        return;
    };
    let (bytes, hash) = common::rust_std_archive();
    let server = common::MockServer::start(bytes);
    let spec = common::rust_spec_for(&server, hash, "rust-std.tar.xz", &release);
    let install_root = common::unique_dir("dl-rust-install");

    let cancel = state.begin_toolchain_download(&spec).expect("start");
    assert_eq!(
        state.toolchain_download_status().toolchain,
        Some(host_core::toolchain_download::Toolchain::Rust),
        "the status names the toolchain that is running"
    );
    let sysroot = state
        .download_toolchain_now(&spec, &install_root, cancel, &mut |_| {})
        .expect("the mock download must succeed");
    println!("sysroot: {}", sysroot.display());
    assert!(sysroot.is_dir(), "a Rust sysroot is a directory");
    assert_eq!(server.hits(), 1);

    // The downloaded sysroot is the active one now — and neither of the other two pins moved.
    assert_eq!(state.rust_config().sysroot, Some(sysroot));
    let settings = std::fs::read_to_string(state.settings_path()).expect("settings.json");
    println!("settings.json: {settings}");
    let parsed: serde_json::Value = serde_json::from_str(&settings).expect("json");
    assert!(parsed["rust_sysroot"].is_string(), "{settings}");
    assert!(
        parsed["toolchain_path"].is_null(),
        "a Rust download must not touch the C pin: {settings}"
    );
    assert!(
        parsed["zig_path"].is_null(),
        "a Rust download must not touch the Zig pin: {settings}"
    );
    assert!(!state.toolchain_download_status().in_progress);
}

/// The version coupling, refused **before** anything is downloaded (v0.9 F3b-2).
#[test]
fn a_rust_download_pinned_to_another_version_is_refused_before_it_starts() {
    let state = state("dl-rust-version");
    let (bytes, hash) = common::rust_std_archive();
    let server = common::MockServer::start(bytes);
    // No machine runs release 0.0.1, so the check has to refuse whichever arm it takes (a
    // mismatch, or no `rustc` to ask at all).
    let spec = common::rust_spec_for(&server, hash, "rust-std.tar.xz", "0.0.1");

    let err = state
        .begin_toolchain_download(&spec)
        .expect_err("a mismatched pin must be refused");
    println!("{err}");
    assert_eq!(server.hits(), 0, "nothing may be downloaded");
    assert!(!state.toolchain_download_status().in_progress);
}

#[test]
fn download_audit_events_are_complete() {
    let state = state("dl-audit");
    let (bytes, hash) = common::toolchain_archive_with_executable();
    let server = common::MockServer::start(bytes);
    let spec = common::spec_for(&server, hash, "toolchain-archive");
    let install_root = unique_dir("dl-audit-install");

    let cancel = state.begin_toolchain_download(&spec).expect("start");
    state
        .download_toolchain_now(&spec, &install_root, cancel, &mut |_| {})
        .expect("download ok");

    let actions = download_actions(&state, "host.toolchain.download.");
    println!("actions: {actions:?}");
    assert!(
        actions.contains(&"host.toolchain.download.start".to_string()),
        "{actions:?}"
    );
    assert!(
        actions.contains(&"host.toolchain.download.done".to_string()),
        "{actions:?}"
    );

    // The detail records the version and the final path, and nothing sensitive.
    let done = state
        .list_events(200, None, Some("host.toolchain.download.done".to_string()))
        .expect("events");
    let detail = done[0].detail.to_string();
    println!("detail: {detail}");
    assert!(detail.contains("xpack-riscv-none-elf-gcc"), "{detail}");
    assert!(
        !detail.to_lowercase().contains("api_key"),
        "no secrets in the audit detail: {detail}"
    );
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
#[ignore = "requires a discoverable QEMU; run with --include-ignored"]
fn run_agent_refuses_without_a_toolchain() {
    let state = state("missingrun");
    // Test seam: force a broken manual toolchain, then try to run.
    *state.toolchain_path.lock().unwrap() = Some(PathBuf::from(
        r"C:\definitely\not\here\riscv64-unknown-elf-gcc.exe",
    ));
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(vec![final_response("hi")])));

    let sink = Arc::new(host_core::events::RecordingEventSink::new());
    let err = state
        .run_agent(sink as Arc<dyn host_core::EventSink>, "hello")
        .expect_err("run must refuse without a toolchain");
    let msg = err.to_string();
    println!("{msg}");
    // The definition's check answers now (v0.9 sandbox F2d): the fallback pins no
    // toolchain, so the refusal carries `sandbox_toolchain_missing` — the switch's
    // own reason code — rather than the host's `toolchain_missing`.
    assert!(msg.contains("sandbox_toolchain_missing"), "{msg}");
}
