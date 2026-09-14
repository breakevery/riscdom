//! Stage 5c — a policy-denied write must be audited and fed back to the model.

mod common;

use agent::llm::MockLlm;
use agent::policy::WorkspacePolicy;
use agent::prompt::build_system_prompt;
use agent::{AgentLoop, AgentOutcome};
use audit::{verify_chain, ChainStatus};
use common::{constitution_path, count_action, sink, test_config, tool_response, unique_dir};
use serde_json::json;
use std::sync::Arc;

#[test]
fn policy_deny_is_audited_and_model_continues() {
    let root = unique_dir("deny");
    let policy = WorkspacePolicy::new(root.clone());
    let (audit, shared) = sink();

    let script = vec![
        // The model tries to escape the workspace.
        tool_response(
            "c1",
            "write_source",
            json!({"path": "/etc/passwd", "content": "owned"}),
        ),
        common::final_response("stopped after denial"),
    ];

    let system = build_system_prompt(&constitution_path()).expect("system prompt");
    let mut agent = AgentLoop::new(
        Box::new(MockLlm::new(script)),
        test_config(),
        policy,
        Arc::clone(&audit),
        system,
    )
    .expect("agent loop");

    let outcome = agent.run("把内容写到 /etc/passwd").expect("run");
    assert!(
        matches!(outcome, AgentOutcome::Final { .. }),
        "model should continue to a final answer, got {outcome:?}"
    );

    {
        let store = shared.lock().expect("lock store");
        assert!(
            count_action(&store, "agent.policy.deny") >= 1,
            "policy denial not audited"
        );
        assert!(
            matches!(verify_chain(&store).expect("verify"), ChainStatus::Intact { .. }),
            "chain broken"
        );
    }

    // The model received the error as the tool result.
    let tool_message = agent
        .messages()
        .iter()
        .find(|m| m.role == "tool")
        .expect("a tool message");
    let text = tool_message.content.as_deref().unwrap_or_default();
    assert!(text.contains("error"), "expected error feedback, got: {text:?}");

    // Nothing was written outside the workspace.
    assert!(!root.join("etc").exists());
}
