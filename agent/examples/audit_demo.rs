//! Demo: write one of each agent audit event and print them (no API key involved).
//!
//! ```text
//! cargo run -p agent --example audit_demo
//! ```

use agent::audit_hook::{
    record_llm_request, record_llm_response, record_policy_deny, record_tool_call,
    record_tool_result,
};
use agent::message::{ChatMessage, ChatRequest, ChatResponse, Choice, FunctionCall, ToolCall, Usage};
use audit::{AuditSink, AuditStore, SqliteAuditSink};
use std::sync::{Arc, Mutex};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let work = std::env::temp_dir().join("riscdom-agent-demo");
    std::fs::create_dir_all(&work)?;
    let db = work.join("audit.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(work.join(format!("audit.db{suffix}")));
    }

    let shared = Arc::new(Mutex::new(AuditStore::open(&db)?));
    let sink: Arc<Mutex<dyn AuditSink>> =
        Arc::new(Mutex::new(SqliteAuditSink::from_shared(Arc::clone(&shared))));

    let req = ChatRequest {
        model: "deepseek-chat".into(),
        messages: vec![ChatMessage::text("user", "写一个 Hello World")],
        tools: Some(vec![serde_json::json!({"type": "function"})]),
        tool_choice: Some("auto".into()),
        temperature: Some(0.0),
        stream: None,
    };
    record_llm_request(&sink, &req, "deepseek-chat");

    let resp = ChatResponse {
        id: Some("resp-1".into()),
        model: Some("deepseek-chat".into()),
        choices: vec![Choice {
            index: Some(0),
            message: ChatMessage::text("assistant", "ok"),
            finish_reason: Some("tool_calls".into()),
        }],
        usage: Some(Usage {
            prompt_tokens: Some(120),
            completion_tokens: Some(40),
            total_tokens: Some(160),
        }),
    };
    record_llm_response(&sink, &resp);

    let call = ToolCall {
        id: "call_1".into(),
        kind: "function".into(),
        function: FunctionCall {
            name: "write_source".into(),
            arguments: "{\"path\":\"hello.c\",\"content\":\"int main(void){}\"}".into(),
        },
    };
    record_tool_call(&sink, &call);
    record_tool_result(&sink, "call_1", "wrote 18 bytes to hello.c", true);
    record_policy_deny(
        &sink,
        "path outside workspace",
        serde_json::json!({ "path": "/etc/passwd" }),
    );

    let store = shared.lock().unwrap();
    println!("audit db: {}", db.display());
    for e in store.all()? {
        println!("{}", serde_json::to_string(&e)?);
    }
    println!("verify: {:?}", audit::verify_chain(&store)?);
    Ok(())
}
