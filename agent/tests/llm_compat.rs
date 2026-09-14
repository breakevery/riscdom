//! Stage 10a — OpenAI-compatible client: validation + config defaults.
//!
//! These tests never touch the network and never use a real key.

use agent::llm::{DeepSeekClient, LlmClient, MockLlm, OpenAiCompatClient};
use agent::{AgentConfig, AgentError};

fn cfg(base_url: &str, api_key: &str, model: &str) -> AgentConfig {
    AgentConfig {
        api_key: api_key.into(),
        base_url: base_url.into(),
        model: model.into(),
        provider_id: "custom".into(),
        max_iterations: 10,
        request_timeout_secs: 120,
    }
}

#[test]
fn cloud_requires_api_key() {
    let missing = cfg("https://api.deepseek.com", "", "deepseek-chat");
    assert!(matches!(missing.validate(), Err(AgentError::Config(_))));

    let ok = cfg("https://api.openai.com/v1", "test-key", "gpt-4o-mini");
    assert!(ok.validate().is_ok());
}

#[test]
fn local_allows_empty_api_key() {
    assert!(cfg("http://localhost:11434/v1", "", "qwen2.5-coder")
        .validate()
        .is_ok());
    assert!(cfg("http://127.0.0.1:1234/v1", "", "local-model")
        .validate()
        .is_ok());
}

#[test]
fn requires_scheme() {
    assert!(cfg("api.deepseek.com", "test-key", "deepseek-chat")
        .validate()
        .is_err());
    assert!(cfg("ftp://example.com", "test-key", "m")
        .validate()
        .is_err());
}

#[test]
fn requires_non_empty_model() {
    assert!(cfg("https://api.deepseek.com", "test-key", "   ")
        .validate()
        .is_err());
}

#[test]
fn deepseek_default_shape() {
    let c = AgentConfig::deepseek_default();
    assert_eq!(c.base_url, "https://api.deepseek.com");
    assert_eq!(c.model, "deepseek-chat");
    assert!(c.api_key.is_empty(), "preset must not embed a key");
    assert_eq!(c.max_iterations, 10);
}

#[test]
fn from_env_defaults_to_deepseek_base_url() {
    // Only this test touches DEEPSEEK_* in this test binary.
    let saved_key = std::env::var("DEEPSEEK_API_KEY").ok();
    let saved_base = std::env::var("DEEPSEEK_BASE_URL").ok();
    let saved_model = std::env::var("DEEPSEEK_MODEL").ok();

    std::env::set_var("DEEPSEEK_API_KEY", "test-key-not-real");
    std::env::remove_var("DEEPSEEK_BASE_URL");
    std::env::remove_var("DEEPSEEK_MODEL");

    let c = AgentConfig::from_env().expect("from_env with key set");
    assert_eq!(c.base_url, "https://api.deepseek.com");
    assert_eq!(c.model, "deepseek-chat");
    assert!(c.validate().is_ok());

    restore("DEEPSEEK_API_KEY", saved_key);
    restore("DEEPSEEK_BASE_URL", saved_base);
    restore("DEEPSEEK_MODEL", saved_model);
}

fn restore(name: &str, value: Option<String>) {
    match value {
        Some(v) => std::env::set_var(name, v),
        None => std::env::remove_var(name),
    }
}

#[test]
fn client_alias_and_trait_object_are_compatible() {
    // `DeepSeekClient` remains a usable alias of `OpenAiCompatClient`.
    let c = cfg("https://api.deepseek.com", "test-key", "deepseek-chat");
    let client: OpenAiCompatClient = OpenAiCompatClient::new(c.clone()).expect("client");
    assert_eq!(
        client.endpoint(),
        "https://api.deepseek.com/chat/completions"
    );
    let _alias = DeepSeekClient::new(c).expect("alias client");

    // Both implementations are usable behind the same trait object.
    let boxed: Vec<Box<dyn LlmClient>> = vec![
        Box::new(
            OpenAiCompatClient::new(cfg("http://localhost:11434/v1", "", "qwen2.5-coder"))
                .expect("local client"),
        ),
        Box::new(MockLlm::new(vec![])),
    ];
    assert_eq!(boxed.len(), 2);
}
