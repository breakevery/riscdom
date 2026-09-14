//! Audit write helpers for the agent.
//!
//! Every LLM request/response, tool call/result and policy denial goes through
//! here so the audit trail is uniform.
//!
//! **Never** put the API key (or any secret) into `detail`.

use audit::{AuditEvent, AuditSink};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use crate::message::{ChatRequest, ChatResponse, ToolCall};

/// Actor name used for all agent-originated audit events.
pub const AUDIT_ACTOR: &str = "agent";

/// Cap for stringified payloads stored in audit detail.
const MAX_DETAIL_STR: usize = 4096;

fn emit(sink: &Arc<Mutex<dyn AuditSink>>, action: &str, detail: serde_json::Value) {
    if let Ok(mut s) = sink.lock() {
        s.record(AuditEvent::new(AUDIT_ACTOR, action, detail));
    }
}

/// Stable non-cryptographic hash (for correlating requests without storing them).
fn short_hash<T: Hash>(value: &T) -> String {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…[truncated {} bytes]", &s[..end], s.len())
    }
}

/// Record an outgoing LLM request. Stores a hash + sizes, never the raw body,
/// and only the **host** of the base URL (never the full URL, never a key).
pub fn record_llm_request(
    sink: &Arc<Mutex<dyn AuditSink>>,
    req: &ChatRequest,
    model: &str,
    base_url: &str,
) {
    emit(
        sink,
        "agent.llm.request",
        serde_json::json!({
            "model": model,
            "base_url_host": host_of(base_url),
            "messages": req.messages.len(),
            "tools": req.tools.as_ref().map(|t| t.len()).unwrap_or(0),
            "request_hash": short_hash(&serde_json::to_string(req).unwrap_or_default()),
        }),
    );
}

/// Extract just the host from a base URL. Drops any scheme, path, port-less
/// userinfo and port — so credentials embedded in a URL never reach the log.
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    authority
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(authority)
        .to_string()
}

/// Record an LLM response (token usage + shape, not the key).
pub fn record_llm_response(sink: &Arc<Mutex<dyn AuditSink>>, resp: &ChatResponse) {
    let usage = resp.usage.clone().unwrap_or_default();
    emit(
        sink,
        "agent.llm.response",
        serde_json::json!({
            "id": resp.id,
            "model": resp.model,
            "finish_reason": resp.choices.first().and_then(|c| c.finish_reason.clone()),
            "tool_calls": resp.tool_calls().map(|t| t.len()).unwrap_or(0),
            "prompt_tokens": usage.prompt_tokens,
            "completion_tokens": usage.completion_tokens,
            "total_tokens": usage.total_tokens,
        }),
    );
}

/// Record a tool call requested by the model.
pub fn record_tool_call(sink: &Arc<Mutex<dyn AuditSink>>, call: &ToolCall) {
    emit(
        sink,
        "agent.tool.call",
        serde_json::json!({
            "id": call.id,
            "name": call.function.name,
            "arguments": truncate(&call.function.arguments, MAX_DETAIL_STR),
            "arguments_len": call.function.arguments.len(),
        }),
    );
}

/// Record the result of a tool call.
pub fn record_tool_result(sink: &Arc<Mutex<dyn AuditSink>>, call_id: &str, result: &str, ok: bool) {
    emit(
        sink,
        "agent.tool.result",
        serde_json::json!({
            "call_id": call_id,
            "ok": ok,
            "result_len": result.len(),
            "result": truncate(result, MAX_DETAIL_STR),
        }),
    );
}

/// Record a capability-policy denial.
pub fn record_policy_deny(
    sink: &Arc<Mutex<dyn AuditSink>>,
    reason: &str,
    detail: serde_json::Value,
) {
    emit(
        sink,
        "agent.policy.deny",
        serde_json::json!({ "reason": reason, "detail": detail }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ChatMessage, Choice, FunctionCall};
    use audit::{verify_chain, AuditStore, ChainStatus, SqliteAuditSink};

    fn sink() -> (Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
        let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
        let s: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::from_shared(
            Arc::clone(&shared),
        )));
        (s, shared)
    }

    #[test]
    fn hooks_write_expected_actions_and_keep_chain_intact() {
        let (sink, shared) = sink();

        let req = ChatRequest {
            model: "deepseek-chat".into(),
            messages: vec![ChatMessage::text("user", "hi")],
            tools: Some(vec![serde_json::json!({"type": "function"})]),
            tool_choice: Some("auto".into()),
            temperature: Some(0.0),
            stream: None,
        };
        record_llm_request(&sink, &req, "deepseek-chat", "https://api.deepseek.com");

        let resp = ChatResponse {
            id: Some("r1".into()),
            model: Some("deepseek-chat".into()),
            choices: vec![Choice {
                index: Some(0),
                message: ChatMessage::text("assistant", "ok"),
                finish_reason: Some("stop".into()),
            }],
            usage: Some(crate::message::Usage {
                prompt_tokens: Some(1),
                completion_tokens: Some(2),
                total_tokens: Some(3),
            }),
        };
        record_llm_response(&sink, &resp);

        let call = ToolCall {
            id: "call_1".into(),
            kind: "function".into(),
            function: FunctionCall {
                name: "write_source".into(),
                arguments: "{\"path\":\"a.c\"}".into(),
            },
        };
        record_tool_call(&sink, &call);
        record_tool_result(&sink, "call_1", "wrote 12 bytes", true);
        record_policy_deny(
            &sink,
            "path outside workspace",
            serde_json::json!({"path": "/etc/passwd"}),
        );

        let store = shared.lock().expect("lock");
        assert_eq!(
            verify_chain(&store).expect("verify"),
            ChainStatus::Intact { length: 5 }
        );
        let actions: Vec<String> = store
            .all()
            .unwrap()
            .into_iter()
            .map(|e| e.event.action)
            .collect();
        for want in [
            "agent.llm.request",
            "agent.llm.response",
            "agent.tool.call",
            "agent.tool.result",
            "agent.policy.deny",
        ] {
            assert!(
                actions.iter().any(|a| a == want),
                "missing {want}: {actions:?}"
            );
        }
    }
}
