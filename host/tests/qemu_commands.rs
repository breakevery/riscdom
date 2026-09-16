//! Stage v0.3-5b-1a — QEMU probe / set / clear through the host state.

use host::state::AppState;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-qemucmd-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn state_with(dir: &PathBuf) -> AppState {
    AppState::in_memory(dir).expect("state")
}

#[test]
fn probe_reports_a_discovered_qemu() {
    let state = state_with(&unique_dir("probe"));
    let view = state.probe_qemu();
    println!(
        "found={} source={} path={:?}",
        view.found, view.source, view.path
    );

    // These host tests boot QEMU, so it must be discoverable.
    assert!(view.found, "expected QEMU: {}", view.diagnostics);
    assert!(
        ["EnvVar", "KnownPath", "Path"].contains(&view.source.as_str()),
        "unexpected source {}",
        view.source
    );
    assert!(view
        .path
        .as_deref()
        .map(|p| p.contains("qemu-system-riscv64"))
        .unwrap_or(false));
    assert!(
        view.diagnostics.contains("RISCDOM_QEMU"),
        "{}",
        view.diagnostics
    );
}

#[test]
fn a_missing_path_is_rejected() {
    let state = state_with(&unique_dir("missing"));
    let err = state
        .set_qemu_path(r"C:\definitely\not\here\qemu-system-riscv64.exe")
        .expect_err("must be rejected");
    assert!(err.to_string().contains("not a file"), "{err}");
}

#[test]
fn a_manual_path_is_stored_and_survives_a_restart() {
    // The same workspace means the same settings file, so a second `AppState`
    // simulates an application restart.
    let workspace = unique_dir("restart");
    let discovered = {
        let state = state_with(&workspace);
        state
            .probe_qemu()
            .path
            .expect("a QEMU must be discoverable")
    };

    {
        let state = state_with(&workspace);
        state.set_qemu_path(&discovered).expect("set");
        let view = state.probe_qemu();
        assert_eq!(view.source, "Manual");
        assert_eq!(view.path.as_deref(), Some(discovered.as_str()));

        let settings = std::fs::read_to_string(state.settings_path()).expect("settings.json");
        println!("settings.json: {settings}");
        assert!(settings.contains("qemu_path"), "{settings}");
    }

    // "restart": the manual path comes back from disk.
    let restarted = state_with(&workspace);
    let view = restarted.probe_qemu();
    assert!(view.found, "{}", view.diagnostics);
    assert_eq!(view.source, "Manual");
    assert_eq!(view.path.as_deref(), Some(discovered.as_str()));

    // Clearing persists too.
    restarted.clear_qemu_path().expect("clear");
    let settings = std::fs::read_to_string(restarted.settings_path()).expect("settings.json");
    assert!(
        settings.contains("\"qemu_path\": null"),
        "qemu_path must be cleared: {settings}"
    );
    let after = state_with(&workspace).probe_qemu();
    assert_ne!(after.source, "Manual");
}

#[test]
fn settings_without_the_new_field_still_load() {
    // Backwards compatibility: an older settings.json has no `qemu_path`.
    let workspace = unique_dir("legacy");
    std::fs::write(
        workspace.join(".riscdom").join("settings.json"),
        br#"{"version": 1, "toolchain_path": null}"#,
    )
    .or_else(|_| {
        std::fs::create_dir_all(workspace.join(".riscdom")).and_then(|_| {
            std::fs::write(
                workspace.join(".riscdom").join("settings.json"),
                br#"{"version": 1, "toolchain_path": null}"#,
            )
        })
    })
    .expect("write legacy settings");

    let state = state_with(&workspace);
    let view = state.probe_qemu();
    assert!(view.found, "legacy settings must not break probing");
    assert_ne!(view.source, "Manual");
}

#[test]
fn run_agent_refuses_without_qemu() {
    let state = state_with(&unique_dir("missingrun"));
    // Test seam: point the manual path at something that is not a file.
    *state.qemu_path.lock().unwrap() = Some(PathBuf::from(
        r"C:\definitely\not\here\qemu-system-riscv64.exe",
    ));

    // The LLM readiness gate runs first, so satisfy it (the run must
    // then fail on the QEMU pre-check).
    *state.llm_override.lock().unwrap() =
        Some(std::sync::Arc::new(agent::llm::MockLlm::new(Vec::new())));
    let sink = std::sync::Arc::new(host::events::RecordingEventSink::new());
    let err = state
        .run_agent(sink as std::sync::Arc<dyn host::EventSink>, "hello")
        .expect_err("run must refuse without QEMU");
    let msg = err.to_string();
    println!("{msg}");
    assert!(msg.starts_with("qemu_missing"), "{msg}");
}
