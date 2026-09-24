//! Stage 17b — sessions wired through `run_agent` + the session commands.

use agent::llm::{LlmClient, MockLlm};
use agent::message::{ChatMessage, ChatRequest, ChatResponse, Choice, FunctionCall, ToolCall};
use host_core::state::AppState;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

fn unique_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-sessint-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn text_response(text: &str) -> ChatResponse {
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

fn tool_response(id: &str, name: &str) -> ChatResponse {
    ChatResponse {
        id: None,
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
                        arguments: json!({}).to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            finish_reason: Some("tool_calls".into()),
        }],
        usage: None,
    }
}

fn state_with(tag: &str, script: Vec<ChatResponse>) -> AppState {
    let state = AppState::in_memory(unique_dir(tag)).expect("state");
    *state.llm_override.lock().unwrap() = Some(Arc::new(MockLlm::new(script)));
    state
}

fn run(state: &AppState, input: &str) {
    let sink = Arc::new(host_core::events::RecordingEventSink::new());
    state
        .run_agent(sink as Arc<dyn host_core::EventSink>, input)
        .expect("run_agent");
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn a_run_creates_a_session_and_persists_the_turn() {
    let state = state_with("create", vec![text_response("hi there")]);

    run(&state, "hello agent");

    let sessions = state.list_sessions(10).expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title, "hello agent");
    assert!(state.current_session_id().is_some());

    let detail = state.open_session(&sessions[0].id).expect("open");
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant"]);
    assert_eq!(detail.messages[0].content, "hello agent");
    assert_eq!(detail.messages[1].content, "hi there");
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn a_second_run_appends_to_the_same_session() {
    let state = state_with(
        "append",
        vec![text_response("first"), text_response("second")],
    );

    run(&state, "turn one");
    let id = state.current_session_id().expect("current");
    run(&state, "turn two");

    assert_eq!(state.current_session_id().as_deref(), Some(id.as_str()));
    let sessions = state.list_sessions(10).expect("list");
    assert_eq!(sessions.len(), 1, "no new session for the second turn");
    assert_eq!(sessions[0].message_count, 4);

    let detail = state.open_session(&id).expect("open");
    let contents: Vec<&str> = detail.messages.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(contents, vec!["turn one", "first", "turn two", "second"]);
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn tool_calls_and_results_are_persisted_as_history() {
    // tool_call → (tool result) → final answer.
    let state = state_with(
        "tools",
        vec![
            tool_response("call_1", "list_workspace"),
            text_response("done"),
        ],
    );

    // `list_workspace` needs no VM, so the whole turn runs offline.
    run(&state, "list the workspace");

    let id = state.current_session_id().expect("current");
    let detail = state.open_session(&id).expect("open");
    let roles: Vec<&str> = detail.messages.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, vec!["user", "assistant", "tool", "assistant"]);

    // The assistant row keeps its tool_calls JSON; the tool row keeps its id.
    let call_row = &detail.messages[1];
    assert!(call_row.tool_call_json.is_some(), "tool_calls JSON missing");
    let tool_row = &detail.messages[2];
    assert_eq!(tool_row.tool_call_id.as_deref(), Some("call_1"));
}

/// Records every request it sees, so tests can inspect the injected history.
struct RecordingLlm {
    seen: Mutex<Vec<Vec<ChatMessage>>>,
    script: Mutex<VecDeque<ChatResponse>>,
}

impl RecordingLlm {
    fn new(script: Vec<ChatResponse>) -> Self {
        Self {
            seen: Mutex::new(Vec::new()),
            script: Mutex::new(script.into()),
        }
    }

    fn requests(&self) -> Vec<Vec<ChatMessage>> {
        self.seen.lock().unwrap().clone()
    }
}

impl LlmClient for RecordingLlm {
    fn chat(&self, req: ChatRequest) -> Result<ChatResponse, agent::AgentError> {
        self.seen.lock().unwrap().push(req.messages.clone());
        self.script
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| agent::AgentError::Other("recording script exhausted".into()))
    }
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn history_is_injected_back_into_the_loop_without_the_system_prompt() {
    let state = AppState::in_memory(unique_dir("history")).expect("state");
    let llm = Arc::new(RecordingLlm::new(vec![
        text_response("first"),
        text_response("second"),
    ]));
    *state.llm_override.lock().unwrap() = Some(Arc::clone(&llm) as Arc<dyn LlmClient>);

    run(&state, "remember me");
    let id = state.current_session_id().unwrap();
    state.open_session(&id).expect("open");
    run(&state, "what did I say?");

    let requests = llm.requests();
    assert_eq!(requests.len(), 2, "one request per run");
    let second = &requests[1];

    // The second request must carry the restored history...
    let contents: Vec<&str> = second.iter().filter_map(|m| m.content.as_deref()).collect();
    assert!(
        contents.contains(&"remember me"),
        "missing history: {contents:?}"
    );
    assert!(contents.contains(&"first"), "missing history: {contents:?}");
    assert!(
        contents.contains(&"what did I say?"),
        "missing new turn: {contents:?}"
    );

    // ...exactly one system prompt (never duplicated or persisted)...
    let system_count = second.iter().filter(|m| m.role == "system").count();
    assert_eq!(system_count, 1, "system prompt should appear once");

    // ...and 4 persisted messages, none of them `system`.
    let detail = state.open_session(&id).expect("open");
    assert_eq!(detail.messages.len(), 4);
    assert!(detail.messages.iter().all(|m| m.role != "system"));
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn rename_and_delete_sessions() {
    let state = state_with("rename", vec![text_response("ok")]);
    run(&state, "original title");
    let id = state.current_session_id().unwrap();

    state.rename_session(&id, "renamed").expect("rename");
    let listed = state.list_sessions(10).expect("list");
    assert_eq!(listed[0].title, "renamed");

    state.delete_session(&id).expect("delete");
    assert!(state.list_sessions(10).expect("list").is_empty());
    assert!(
        state.current_session_id().is_none(),
        "deleting the current session clears it"
    );

    // A fresh run starts a new session.
    run(&state, "after delete");
    assert_eq!(state.list_sessions(10).unwrap().len(), 1);
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn clear_all_sessions_removes_everything() {
    let state = state_with("clear", vec![text_response("ok")]);
    run(&state, "one");
    state.create_session("manual").expect("create");
    assert_eq!(state.list_sessions(10).unwrap().len(), 2);

    state.clear_all_sessions().expect("clear");
    assert!(state.list_sessions(10).unwrap().is_empty());
    assert!(state.current_session_id().is_none());
}
