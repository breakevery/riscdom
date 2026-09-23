//! v0.9 sandbox F2d — which definition a run uses, and what a declaration may not do.
//!
//! The resolution order is the part worth testing without a guest: a task outranks
//! the node, the node outranks its default, and a name nobody has is refused rather
//! than quietly replaced. The one case a test cannot reach from outside is
//! `current_sandbox` — nothing but a successful switch writes it, and a switch boots
//! a guest — so the node-over-default step is pinned by the unit test inside
//! `state.rs` and the rest here.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice};
use host_core::events::RecordingEventSink;
use host_core::state::AppState;
use host_core::{EventSink, HostError};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-task-sandbox-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workspace whose `settings.json` already carries hand-written definitions.
///
/// Written **before** the state is built, because `AppState` reads its settings at
/// construction: the registry a resolver sees is the registry that was on disk.
fn workspace_with(tag: &str, settings: serde_json::Value) -> PathBuf {
    let root = unique_dir(tag);
    let host_dir = root.join(".riscdom");
    std::fs::create_dir_all(&host_dir).unwrap();
    // `LocalSettings::load` falls back to defaults for a malformed file, and
    // `version` has no serde default: a fixture without it would silently load as
    // "no definitions at all", and every test below would pass for the wrong
    // reason. So the version is part of the fixture, not of each test.
    let mut settings = settings;
    if settings.get("version").is_none() {
        settings["version"] = serde_json::json!(1);
    }
    std::fs::write(
        host_dir.join("settings.json"),
        serde_json::to_vec_pretty(&settings).expect("settings json"),
    )
    .unwrap();
    root
}

fn definition(
    name: &str,
    qemu: Option<String>,
    memory_mb: Option<u32>,
    toolchain: Option<String>,
) -> serde_json::Value {
    let mut def = serde_json::json!({ "name": name });
    if let Some(qemu) = qemu {
        def["qemu_exe"] = serde_json::json!(qemu);
    }
    if let Some(toolchain) = toolchain {
        def["toolchain_path"] = serde_json::json!(toolchain);
    }
    if let Some(memory_mb) = memory_mb {
        def["memory_mb"] = serde_json::json!(memory_mb);
    }
    def
}

/// An LLM that answers once with a final message, so a run can be driven hermetically.
fn scripted_llm(text: &str) -> Box<dyn agent::LlmClient> {
    Box::new(MockLlm::new(vec![ChatResponse {
        id: Some("resp-final".into()),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }]))
}

#[test]
fn a_task_that_names_a_sandbox_gets_it() {
    let root = workspace_with(
        "named",
        serde_json::json!({
            "sandboxes": [definition("blink", None, Some(256), None)],
            "default_sandbox": "blink",
        }),
    );
    let state = AppState::in_memory(&root).expect("state");
    let (def, name) = state
        .resolve_task_sandbox(Some("blink"))
        .expect("resolved")
        .expect("a definition");
    assert_eq!(name, "blink");
    assert_eq!(def.memory_mb, Some(256));
}

#[test]
fn a_task_that_names_none_gets_the_nodes_default() {
    let root = workspace_with(
        "default",
        serde_json::json!({
            "sandboxes": [definition("blink", None, Some(256), None)],
            "default_sandbox": "blink",
        }),
    );
    let state = AppState::in_memory(&root).expect("state");
    // Nothing is current, so the configured default answers.
    assert_eq!(state.current_sandbox(), None);
    let (def, name) = state
        .resolve_task_sandbox(None)
        .expect("resolved")
        .expect("a definition");
    assert_eq!(name, "blink");
    assert_eq!(def.memory_mb, Some(256));
}

#[test]
fn with_no_settings_at_all_the_fallback_answers() {
    // The registry always carries the built-in fallback, so a run always resolves
    // *something* — that definition has no paths, so the host goes on discovering
    // its own QEMU and toolchain, which is what it did before this batch.
    let state = AppState::in_memory(unique_dir("fallback")).expect("state");
    let (def, name) = state
        .resolve_task_sandbox(None)
        .expect("resolved")
        .expect("the fallback");
    assert_eq!(name, host_core::DEFAULT_SANDBOX_NAME);
    assert_eq!(def.qemu_exe, None);
    assert_eq!(def.toolchain_path, None);
    assert_eq!(def.memory_mb, None);
}

#[test]
fn an_unknown_name_is_refused_rather_than_replaced() {
    let root = workspace_with(
        "unknown",
        serde_json::json!({
            "sandboxes": [definition("blink", None, Some(256), None)],
            "default_sandbox": "blink",
        }),
    );
    let state = AppState::in_memory(&root).expect("state");
    let err = state
        .resolve_task_sandbox(Some("nonexistent"))
        .expect_err("refused");
    // Exactly the switch's answer for a name nobody has: the caller's parameter.
    assert!(matches!(err, HostError::SandboxNotFound(_)), "{err}");
    assert!(
        err.to_string().contains("nonexistent"),
        "the refusal names the name: {err}"
    );
}

#[test]
fn a_stale_default_name_falls_through_to_the_fallback() {
    // `default_sandbox` can name a definition that no longer exists (the file was
    // edited after a switch). A run nobody asked to be picky about should still
    // start, on the fallback.
    let root = workspace_with("stale", serde_json::json!({ "default_sandbox": "ghost" }));
    let state = AppState::in_memory(&root).expect("state");
    let (_, name) = state
        .resolve_task_sandbox(None)
        .expect("resolved")
        .expect("the fallback");
    assert_eq!(name, host_core::DEFAULT_SANDBOX_NAME);
}

#[test]
fn a_declared_sandbox_reaches_the_loop_and_its_broken_definition_refuses_the_run() {
    // The hermetic proof that the declaration was *used*: the definition's QEMU does
    // not exist, so a run under it stops before anything is started — with the
    // reason code the switch would give. No guest, no network.
    let missing = unique_dir("missing-qemu").join("no-such-qemu");
    let root = workspace_with(
        "injected",
        serde_json::json!({
            "sandboxes": [definition("broken", Some(missing.display().to_string()), None, None)],
            "default_sandbox": "broken",
        }),
    );
    let state = AppState::in_memory(&root).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::from(scripted_llm("done")));
    let sink = Arc::new(RecordingEventSink::new());

    let err = state
        .run_agent_for(sink as Arc<dyn EventSink>, "hello", Some("broken"))
        .expect_err("the definition cannot run");
    assert!(matches!(err, HostError::SandboxQemuMissing(_)), "{err}");
    assert!(err.to_string().starts_with("sandbox_qemu_missing"), "{err}");
    // Nothing was started, so nothing claims to be running.
    assert_eq!(state.active_sandbox(), None);
}

#[test]
fn a_run_that_declares_nothing_still_resolves_the_node_default() {
    // The default definition has no pinned QEMU, so it asks the *host* — and with
    // no toolchain configured the run is refused by the definition's own check,
    // which is the same thing the node-level pre-check would have said.
    let missing = unique_dir("pinned-qemu").join("no-such-qemu");
    let root = workspace_with(
        "default-pinned",
        serde_json::json!({
            "sandboxes": [definition("broken", Some(missing.display().to_string()), None, None)],
            "default_sandbox": "broken",
        }),
    );
    let state = AppState::in_memory(&root).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::from(scripted_llm("done")));
    let sink = Arc::new(RecordingEventSink::new());

    let err = state
        .run_agent(sink as Arc<dyn EventSink>, "hello")
        .expect_err("the default definition cannot run");
    assert!(matches!(err, HostError::SandboxQemuMissing(_)), "{err}");
}

#[test]
fn a_declared_sandbox_does_not_move_the_node() {
    // No VM is running, so the declaration is honoured — and the node's own sandbox
    // is untouched by the attempt either way (F2d decision 1).
    let root = workspace_with(
        "no-move",
        serde_json::json!({
            "sandboxes": [definition("blink", None, Some(64), None)],
            "default_sandbox": "blink",
        }),
    );
    let state = AppState::in_memory(&root).expect("state");
    assert_eq!(state.current_sandbox(), None);
    let _ = state.run_agent_for(
        Arc::new(RecordingEventSink::new()) as Arc<dyn EventSink>,
        "hello",
        Some("blink"),
    );
    assert_eq!(
        state.current_sandbox(),
        None,
        "a declaration moved the node"
    );
    assert_eq!(state.sandbox_default_name(), "blink", "the default changed");
}

/// The `settings.json` helper is worth one check of its own: if this were wrong,
/// every test above would pass for the wrong reason.
#[test]
fn the_settings_fixture_is_what_the_registry_reads() {
    let root = workspace_with(
        "fixture",
        serde_json::json!({
            "sandboxes": [definition("blink", None, Some(256), None)],
            "default_sandbox": "blink",
        }),
    );
    let state = AppState::in_memory(&root).expect("state");
    let names: Vec<String> = state.sandboxes().into_iter().map(|v| v.name).collect();
    assert!(names.contains(&"blink".to_string()), "{names:?}");
    assert_eq!(state.sandbox_default_name(), "blink");
    assert!(Path::new(&root)
        .join(".riscdom")
        .join("settings.json")
        .is_file());
}
