//! Stage 12c — the agent loop refuses to start on an unusable config.

mod common;

use agent::llm::MockLlm;
use agent::policy::WorkspacePolicy;
use agent::{AgentConfig, AgentLoop, AgentOutcome};
use audit::{AuditSink, AuditStore, SqliteAuditSink};
use std::sync::{Arc, Mutex};

fn loop_with(config: AgentConfig) -> AgentLoop {
    let policy = WorkspacePolicy::new(common::unique_dir("readiness"));
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let sink: Arc<Mutex<dyn AuditSink>> =
        Arc::new(Mutex::new(SqliteAuditSink::from_shared(shared)));
    // An empty script means MockLlm would error if it were ever called.
    AgentLoop::new(
        Box::new(MockLlm::new(vec![])),
        config,
        policy,
        sink,
        "system".into(),
        agent::next_agent_id(),
    )
    .expect("agent loop")
}

fn base_config() -> AgentConfig {
    AgentConfig {
        api_key: "test-key".into(),
        base_url: "https://api.deepseek.com".into(),
        model: "deepseek-chat".into(),
        provider_id: "deepseek".into(),
        max_iterations: 10,
        request_timeout_secs: 30,
    }
}

#[test]
fn keyless_cloud_config_is_rejected_before_any_llm_call() {
    let mut config = base_config();
    config.api_key = String::new();

    let mut agent = loop_with(config);
    let outcome = agent.run("hello").expect("run returns Ok(Failed)");

    match outcome {
        AgentOutcome::Failed { reason, iterations } => {
            assert_eq!(iterations, 0, "must fail before the first iteration");
            let lower = reason.to_lowercase();
            assert!(
                lower.contains("api_key") || lower.contains("api key"),
                "reason should mention the api key, got: {reason}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn invalid_base_url_is_rejected_before_any_llm_call() {
    let mut config = base_config();
    config.base_url = "api.deepseek.com".into(); // no scheme

    let mut agent = loop_with(config);
    let outcome = agent.run("hello").expect("run returns Ok(Failed)");

    match outcome {
        AgentOutcome::Failed { reason, iterations } => {
            assert_eq!(iterations, 0);
            let lower = reason.to_lowercase();
            assert!(
                lower.contains("base_url") || lower.contains("base url"),
                "reason should mention the base url, got: {reason}"
            );
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}
