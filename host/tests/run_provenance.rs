//! v0.4 batch 1c — host run provenance.
//!
//! One `run_agent` call is one run: the host mints the id, writes `run.start` /
//! `run.end` into the hash chain (with the canonical configuration JSON on the
//! start event) and maintains the derived index. A snapshot restore is its own
//! run. The id never reaches the agent or the sandbox.

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use audit::{verify_chain, ChainStatus, RunStatus};
use host::events::RecordingEventSink;
use host::state::{AppState, LlmConfigInput};
use host::EventSink;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

const HELLO_C: &str = include_str!("../../agent/tests/fixtures/hello.c");

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-provenance-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn tool_response(id: &str, name: &str, args: serde_json::Value) -> ChatResponse {
    ChatResponse {
        id: Some(format!("resp-{id}")),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage {
                role: "assistant".into(),
                content: None,
                tool_calls: Some(vec![ToolCall {
                    id: id.to_string(),
                    kind: "function".into(),
                    function: FunctionCall {
                        name: name.to_string(),
                        arguments: args.to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            finish_reason: Some("tool_calls".into()),
        }],
        usage: None,
    }
}

fn final_response(text: &str) -> ChatResponse {
    ChatResponse {
        id: Some("resp-final".into()),
        model: Some("mock".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", text),
            finish_reason: Some("stop".into()),
        }],
        usage: None,
    }
}

/// write_source → compile → start_vm → read_serial → final.
fn boot_script() -> Vec<ChatResponse> {
    vec![
        tool_response(
            "c1",
            "write_source",
            json!({ "path": "hello.c", "content": HELLO_C }),
        ),
        tool_response(
            "c2",
            "compile",
            json!({ "source_path": "hello.c", "output_elf": "hello.elf" }),
        ),
        tool_response("c3", "start_vm", json!({ "elf_path": "hello.elf" })),
        tool_response("c4", "read_serial", json!({})),
        final_response("booted"),
    ]
}

/// An LLM client that always fails, for the run-level error path.
struct FailingLlm;

impl agent::LlmClient for FailingLlm {
    fn chat(&self, _req: agent::ChatRequest) -> Result<ChatResponse, agent::AgentError> {
        Err(agent::AgentError::Api("boom".into()))
    }
}

fn state_with(tag: &str, script: Vec<ChatResponse>) -> (AppState, Arc<RecordingEventSink>) {
    let state = AppState::in_memory(unique_dir(tag)).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn host::EventSink>)
        .expect("forwarder");
    (state, sink)
}

fn runs(state: &AppState) -> Vec<audit::RunRecord> {
    state
        .audit
        .lock()
        .expect("audit")
        .list_runs(50)
        .expect("list")
}

fn chain_status(state: &AppState) -> ChainStatus {
    verify_chain(&state.audit.lock().expect("audit")).expect("verify")
}

#[test]
fn a_successful_run_writes_a_matching_start_and_end_pair() {
    let (state, sink) = state_with("ok", boot_script());
    state
        .run_agent(sink as Arc<dyn EventSink>, "boot it")
        .expect("run");

    let runs = runs(&state);
    assert_eq!(runs.len(), 1, "one run_agent call, one run");
    let run = &runs[0];
    assert!(run.run_id.starts_with("run_"), "id: {}", run.run_id);
    assert_eq!(run.status, RunStatus::Ok);
    assert!(run.end_seq.expect("closed") > run.start_seq);
    assert!(
        run.ended_at_ms.expect("ended") >= run.started_at_ms,
        "end before start: {run:?}"
    );
    assert_eq!(
        run.fingerprint,
        audit::fingerprint(&state.run_fingerprint()),
        "the index row must carry the digest of the fingerprint the host reports"
    );
    assert!(run.session_id.is_some(), "runs belong to a session");

    // The markers are ordinary chained events, and the chain stays valid.
    match chain_status(&state) {
        ChainStatus::Intact { length } => assert!(length > 2, "length: {length}"),
        other => panic!("chain broken: {other:?}"),
    }
    assert!(
        state
            .audit
            .lock()
            .unwrap()
            .check_run_index()
            .expect("check")
            .is_empty(),
        "the derived index must agree with the chain"
    );
}

#[test]
fn a_failing_run_is_closed_as_failed() {
    let (state, sink) = state_with("failed", vec![]);
    *state.llm_override.lock().unwrap() = Some(Arc::new(FailingLlm));

    // The host reports an agent-level failure in the outcome, not as an error.
    let view = state
        .run_agent(sink as Arc<dyn EventSink>, "this will fail")
        .expect("the host returns the failed outcome");
    assert_eq!(view.kind, "failed", "precondition: the agent run failed");
    println!("outcome: {view:?}");

    let runs = runs(&state);
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].status,
        RunStatus::Failed,
        "a failed run is closed as failed, not as ok and not left open"
    );
    assert!(
        runs[0].end_seq.is_some(),
        "the error path still closes the run"
    );
    assert!(matches!(chain_status(&state), ChainStatus::Intact { .. }));
}

#[test]
fn a_run_that_never_started_writes_no_run() {
    // A configuration the readiness gate rejects: `run_agent` returns before any
    // run exists, so no start marker and no index row are written.
    let state = AppState::in_memory(unique_dir("gate")).expect("state");
    state.set_llm_config(LlmConfigInput {
        provider_id: "custom".into(),
        api_key: String::new(),
        base_url: "https://example.invalid/v1".into(),
        model: "m".into(),
    });
    assert!(
        !state.llm_readiness().ready,
        "precondition: this configuration must not be ready"
    );

    let sink = Arc::new(RecordingEventSink::new());
    state
        .run_agent(sink as Arc<dyn EventSink>, "no model configured")
        .expect_err("the readiness gate must reject");

    assert!(
        runs(&state).is_empty(),
        "provenance must not fabricate a run for a run that never started"
    );
}

#[test]
fn an_unfinished_run_keeps_a_null_end_seq() {
    // Simulates a run whose process disappeared: the start marker is in the
    // chain, nothing ever closed it, and nothing may invent an end.
    let state = AppState::in_memory(unique_dir("open")).expect("state");
    let config = state.run_fingerprint();
    let detail = audit::run_start_detail("run_orphan", Some("s9"), None, None, &config);
    {
        let mut store = state.audit.lock().unwrap();
        let stored = store
            .append(audit::AuditEvent::new(
                "host",
                audit::ACTION_RUN_START,
                detail,
            ))
            .expect("start");
        // The host maintains the index the same way; only the end is missing,
        // because the run never came back.
        store
            .index_run_start(&audit::RunRecord {
                run_id: "run_orphan".into(),
                session_id: Some("s9".into()),
                parent_run_id: None,
                resumed_from_snapshot: None,
                fingerprint: audit::fingerprint(&config),
                fingerprint_schema: audit::FINGERPRINT_SCHEMA_V1.into(),
                started_at_ms: stored.event.timestamp_ms,
                ended_at_ms: None,
                start_seq: stored.id,
                end_seq: None,
                status: RunStatus::Open,
            })
            .expect("index");
    }

    let runs = runs(&state);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Open);
    assert_eq!(runs[0].end_seq, None, "no fabricated end");
    assert_eq!(runs[0].ended_at_ms, None);

    // And the chain view agrees: the run is open, with no invented end event.
    let (derived, report) = {
        let store = state.audit.lock().unwrap();
        store.derive_runs().expect("derive")
    };
    assert_eq!(derived.len(), 1);
    assert_eq!(derived[0].end_seq, None);
    assert_eq!(report.starts, 1);
    assert_eq!(report.ends, 0);
}

#[test]
fn a_snapshot_restore_opens_a_new_run_linked_to_its_producer() {
    let (state, sink) = state_with("restore", boot_script());
    state
        .run_agent(sink.clone() as Arc<dyn EventSink>, "boot it")
        .expect("run");
    let producer = runs(&state)[0].run_id.clone();

    state.save_snapshot_real("s1c").expect("save");
    state.resume_from_snapshot_real("s1c").expect("restore");

    let runs = runs(&state);
    assert_eq!(runs.len(), 2, "a restore is its own run");
    let restored = &runs[1];
    assert_ne!(restored.run_id, producer);
    assert_eq!(
        restored.parent_run_id.as_deref(),
        Some(producer.as_str()),
        "the restore links to the run that produced the snapshot"
    );
    assert_eq!(restored.status, RunStatus::Ok);
    assert!(restored.end_seq.is_some());
    // v0.5 batch 3: the derived index carries the source snapshot, so the run list
    // can say "restored from s1c" without reading the chain itself.
    assert_eq!(
        restored.resumed_from_snapshot.as_deref(),
        Some("s1c"),
        "the index names the snapshot the restore came from"
    );
    assert_eq!(
        runs[0].resumed_from_snapshot, None,
        "a run that started from scratch has no source snapshot"
    );

    // The same value is on the chain, read back from the restore run's start event:
    // the index is derived, so the two must agree.
    let events = state
        .audit
        .lock()
        .unwrap()
        .list(audit::EventFilter::default(), 500)
        .expect("list");
    let start = events
        .iter()
        .find(|e| {
            e.event.action == audit::ACTION_RUN_START
                && e.event.detail.get("run_id").and_then(|v| v.as_str())
                    == Some(restored.run_id.as_str())
        })
        .expect("restore run.start");
    let payload = audit::parse_run_start(&start.event.detail).expect("parse");
    assert_eq!(payload.resumed_from_snapshot.as_deref(), Some("s1c"));
    assert_eq!(payload.parent_run_id.as_deref(), Some(producer.as_str()));

    // The snapshot JSON travels in the chain, so the digest can be recomputed.
    let recovered: serde_json::Value =
        serde_json::from_str(&payload.fingerprint_json).expect("json");
    assert_eq!(audit::fingerprint(&recovered), payload.fingerprint);

    assert!(matches!(chain_status(&state), ChainStatus::Intact { .. }));
    assert!(state
        .audit
        .lock()
        .unwrap()
        .check_run_index()
        .expect("check")
        .is_empty());
}

#[test]
fn the_fingerprint_is_stable_for_the_same_config_and_differs_otherwise() {
    let state = AppState::in_memory(unique_dir("fingerprint")).expect("state");

    let first = state.run_fingerprint();
    let second = state.run_fingerprint();
    assert_eq!(
        audit::fingerprint(&first),
        audit::fingerprint(&second),
        "the same configuration must fingerprint identically"
    );

    // A different model changes the fingerprint, and only along `llm.model`.
    state.set_llm_config(LlmConfigInput {
        provider_id: "custom".into(),
        api_key: "test-key-must-not-leak".into(),
        base_url: "https://example.invalid/v1".into(),
        model: "some-other-model".into(),
    });
    let third = state.run_fingerprint();
    assert_ne!(audit::fingerprint(&first), audit::fingerprint(&third));
    assert_eq!(first["llm"]["model"], json!("deepseek-chat"));
    assert_eq!(third["llm"]["model"], json!("some-other-model"));

    // Secrets are absent, not hashed: the key never appears in the document.
    let text = audit::canonical_json(&third);
    assert!(
        !text.contains("test-key-must-not-leak"),
        "the API key must not be part of the fingerprint"
    );

    // The prompt is represented by its digest only.
    let prompt_hash = third["prompt"]["sha256"].as_str().expect("prompt hash");
    assert_eq!(prompt_hash.len(), 64, "sha256 hex");
    assert!(
        !text.contains("## Your role"),
        "prompt text must not be stored"
    );
}

#[test]
fn the_fingerprint_reads_the_machine_cpu_and_crt0_from_their_owners() {
    // The values are not copies kept in the host: a change in the sandbox or the
    // compiler reaches the fingerprint (v0.4 1e).
    let state = AppState::in_memory(unique_dir("authority")).expect("state");
    let fp = state.run_fingerprint();
    assert_eq!(fp["vm"]["machine"], json!(sandbox::VM_MACHINE));
    assert_eq!(fp["vm"]["cpu"], json!(sandbox::VM_CPU));
    assert_eq!(fp["vm"]["memory_mb"], json!(agent::VM_MEMORY_MB));
    assert_eq!(fp["agent"]["crt0"], json!(agent::CRT0_INJECTED));
}

#[test]
fn the_run_id_never_reaches_the_agent() {
    let (state, sink) = state_with("noid", boot_script());
    state
        .run_agent(sink as Arc<dyn EventSink>, "boot it")
        .expect("run");
    let run_id = runs(&state)[0].run_id.clone();

    let events = state
        .audit
        .lock()
        .unwrap()
        .list(audit::EventFilter::default(), 500)
        .expect("list");
    for stored in &events {
        let text = stored.event.detail.to_string();
        if stored.event.actor == "host" {
            continue;
        }
        assert!(
            !text.contains(&run_id),
            "the run id leaked into a `{}` event: {text}",
            stored.event.action
        );
    }
    // The markers themselves do carry it (that is their whole point).
    let marker_hits = events
        .iter()
        .filter(|e| e.event.detail.to_string().contains(&run_id))
        .count();
    assert_eq!(marker_hits, 2, "exactly run.start and run.end");
}
