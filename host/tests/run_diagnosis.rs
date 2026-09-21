//! Stage 5c-3 — the end-to-end failure-path diagnosis.
//!
//! Pins what a failing run's report says: which step broke and what it said,
//! whether the serial console ever produced anything, and that nothing is invented
//! when the run actually went fine.

mod common;
mod diagnosis;

use agent::llm::MockLlm;
use agent::message::{ChatMessage, ChatResponse, Choice, FunctionCall, ToolCall};
use diagnosis::{RunDiagnosis, ToolStep};
use host::events::RecordingEventSink;
use host::state::AppState;
use host::StoredEventView;
use serde_json::json;
use std::sync::Arc;

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

/// An LLM client that always fails (the run-level error path).
struct FailingLlm;

impl agent::LlmClient for FailingLlm {
    fn chat(&self, _req: agent::ChatRequest) -> Result<ChatResponse, agent::AgentError> {
        Err(agent::AgentError::Api("boom".into()))
    }
}

fn stored(id: i64, action: &str, detail: serde_json::Value) -> StoredEventView {
    StoredEventView {
        id,
        timestamp_ms: 1_700_000_000_000 + id,
        actor: "agent".into(),
        action: action.into(),
        detail,
        prev_hash: "0".repeat(64),
        hash: format!("{id:064}"),
        agent_id: None,
    }
}

fn state_with(tag: &str, script: Vec<ChatResponse>) -> (AppState, Arc<RecordingEventSink>) {
    let state = AppState::in_memory(common::unique_dir(tag)).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));
    let sink = Arc::new(RecordingEventSink::new());
    state
        .start_serial_forwarder(sink.clone() as Arc<dyn host::EventSink>)
        .expect("serial forwarder");
    (state, sink)
}

#[test]
fn the_report_names_the_failing_tool_step() {
    // A policy-denied write is a real end-to-end failure that needs no QEMU.
    let (state, sink) = state_with(
        "diag-denied",
        vec![
            tool_response(
                "c1",
                "write_source",
                json!({ "path": "evil.py", "content": "x" }),
            ),
            final_response("I could not write that file."),
        ],
    );

    let outcome = state
        .run_agent(sink.clone() as Arc<dyn host::EventSink>, "写一个 .py 文件")
        .expect("the host reports the refusal in the outcome");
    let report = diagnosis::report(&state, &sink, Some(&outcome), None);
    println!("{report}");

    assert!(
        report.contains("first failure: tool `write_source` failed"),
        "the failed step must be named first: {report}"
    );
    assert!(
        report.contains("evil.py") || report.contains("denied"),
        "the tool's own message must be quoted: {report}"
    );
    assert!(
        report.contains("[ERR] write_source"),
        "the step list must mark it: {report}"
    );
    assert!(
        report.contains("serial     : 0 byte(s)"),
        "no guest ran, and the report must say so: {report}"
    );
    assert!(
        report.contains("outcome    : final"),
        "a run can finish while a step failed: {report}"
    );
}

#[test]
fn a_failed_run_reports_reason_and_empty_serial() {
    let (state, sink) = state_with("diag-failed", vec![]);
    *state.llm_override.lock().unwrap() = Some(Arc::new(FailingLlm));

    let outcome = state
        .run_agent(sink.clone() as Arc<dyn host::EventSink>, "this will fail")
        .expect("the host returns the failed outcome");
    assert_eq!(outcome.kind, "failed");
    let report = diagnosis::report(&state, &sink, Some(&outcome), None);
    println!("{report}");

    assert!(report.contains("outcome    : failed after"), "{report}");
    assert!(
        report.contains("the run ended as `failed`"),
        "the run's own reason must lead: {report}"
    );
    assert!(
        report.contains("steps      : none"),
        "no tool ever ran: {report}"
    );
    assert!(
        report.contains("the guest never printed anything"),
        "{report}"
    );
}

#[test]
fn a_host_refusal_is_reported_as_such() {
    let state = AppState::in_memory(common::unique_dir("diag-refused")).expect("state");
    let sink = Arc::new(RecordingEventSink::new());
    let report = diagnosis::report(
        &state,
        &sink,
        None,
        Some("qemu_missing\nQEMU (qemu-system-riscv64) not found."),
    );
    println!("{report}");
    assert!(report.contains("outcome    : (no outcome)"), "{report}");
    assert!(
        report.contains("first failure: the host refused the run: qemu_missing"),
        "{report}"
    );
}

#[test]
fn tool_calls_are_paired_with_their_results() {
    // Real detail shapes: the call carries `id`/`name`, the result `call_id`/`ok`.
    let events = vec![
        stored(
            1,
            "agent.tool.call",
            json!({ "id": "tool-1-compile", "name": "compile", "arguments": "{}" }),
        ),
        stored(
            2,
            "agent.tool.result",
            json!({ "call_id": "tool-1-compile", "ok": false, "result": "boom" }),
        ),
        stored(
            3,
            "agent.tool.call",
            json!({ "id": "tool-2-read", "name": "read_serial", "arguments": "{}" }),
        ),
        stored(
            4,
            "agent.tool.result",
            json!({ "call_id": "tool-2-read", "ok": true, "result": "HELLO RISCV" }),
        ),
    ];

    let steps = diagnosis::tool_steps(&events);
    assert_eq!(steps.len(), 2, "{steps:?}");
    assert_eq!(
        steps[0],
        ToolStep {
            name: "compile".into(),
            ok: false,
            detail: "boom".into()
        }
    );
    assert_eq!(steps[1].name, "read_serial");
    assert!(steps[1].ok);

    // The host hands events over newest first; the report must not care.
    let mut reversed = events.clone();
    reversed.reverse();
    assert_eq!(diagnosis::tool_steps(&reversed), steps);
}

#[test]
fn the_renderer_invents_nothing() {
    let steps = vec![ToolStep {
        name: "compile".into(),
        ok: false,
        detail: "compile failed:\nhello.c:3: error: 'WRONG' undeclared".into(),
    }];
    let counts = vec![("agent:final".to_string(), 0usize)];
    let failing = RunDiagnosis {
        outcome_kind: "final",
        outcome_reason: None,
        iterations: 3,
        steps: &steps,
        serial_bytes: 0,
        serial_tail: "",
        vm_running: false,
        chain: "Intact { length: 42 }",
        events: &counts,
        host_error: None,
    };
    let report = diagnosis::render(&failing);
    assert!(
        report.contains("first failure: tool `compile` failed: compile failed:"),
        "the report quotes the tool's first line: {report}"
    );
    assert!(
        report.contains("[ok ] ") || report.contains("[ERR] compile"),
        "{report}"
    );
    assert!(report.contains("Intact { length: 42 }"), "{report}");
    assert!(report.contains("agent:final x0"), "{report}");

    // A run where nothing failed says so, instead of implying a problem.
    let clean = RunDiagnosis {
        steps: &[],
        outcome_kind: "final",
        events: &[],
        ..failing.clone()
    };
    let clean_report = diagnosis::render(&clean);
    assert!(
        clean_report.contains("first failure: (none)"),
        "{clean_report}"
    );
    assert!(clean_report.contains("steps      : none"), "{clean_report}");
}
