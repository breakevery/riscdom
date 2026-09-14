//! Stage 24c — toolchain probe / set / clear through the host state.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice};
use host::state::AppState;
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
fn run_agent_refuses_without_a_toolchain() {
    let state = state("missingrun");
    // Test seam: force a broken manual toolchain, then try to run.
    *state.toolchain_path.lock().unwrap() = Some(PathBuf::from(
        r"C:\definitely\not\here\riscv64-unknown-elf-gcc.exe",
    ));
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(vec![final_response("hi")])));

    let sink = Arc::new(host::events::RecordingEventSink::new());
    let err = state
        .run_agent(sink as Arc<dyn host::EventSink>, "hello")
        .expect_err("run must refuse without a toolchain");
    let msg = err.to_string();
    println!("{msg}");
    assert!(msg.starts_with("toolchain_missing"), "{msg}");
}
