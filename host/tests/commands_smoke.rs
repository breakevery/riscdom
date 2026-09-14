//! Stage 6a — host command/state smoke tests (no Tauri, no network).

use host::state::{AppState, LlmConfigInput};
use host::{AuditStatusView, ChainStatusView};

fn unique_ws(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("riscdom-host-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn audit_status_starts_intact() {
    let state = AppState::in_memory(unique_ws("audit")).expect("state");
    let status: AuditStatusView = state.audit_status().expect("audit status");
    assert_eq!(status.count, 0);
    match status.chain {
        ChainStatusView::Intact { length } => assert_eq!(length, 0),
        other => panic!("expected Intact, got {other:?}"),
    }
}

#[test]
fn llm_config_roundtrip_and_no_key_leak() {
    let state = AppState::in_memory(unique_ws("llm")).expect("state");

    assert!(!state.llm_config_status().configured);

    state.set_llm_config(LlmConfigInput {
        provider_id: "deepseek".into(),
        api_key: "placeholder-key".into(),
        base_url: "https://api.deepseek.com".into(),
        model: "deepseek-chat".into(),
    });

    let status = state.llm_config_status();
    assert!(status.configured);
    assert_eq!(status.base_url, "https://api.deepseek.com");
    assert_eq!(status.model, "deepseek-chat");

    // The serialised status must not contain the key.
    let json = serde_json::to_string(&status).unwrap();
    assert!(!json.contains("placeholder-key"), "key leaked: {json}");
    assert!(!json.to_lowercase().contains("api_key"), "api_key field leaked: {json}");

    state.clear_llm_config();
    assert!(!state.llm_config_status().configured);
}

#[test]
fn workspace_file_reads_are_policy_checked() {
    let ws = unique_ws("files");
    std::fs::write(ws.join("hello.c"), "int main(void){return 0;}").unwrap();
    let state = AppState::in_memory(&ws).expect("state");

    let files = state.workspace_files().expect("files");
    assert!(files.contains(&"hello.c".to_string()), "{files:?}");

    let text = state.read_workspace_file("hello.c".into()).expect("read");
    assert!(text.contains("main"));

    // Escaping the workspace is denied.
    assert!(state.read_workspace_file("../secret.txt".into()).is_err());
}

#[test]
fn provider_presets_are_available() {
    let state = AppState::in_memory(unique_ws("presets")).expect("state");
    let presets = state.provider_presets();
    assert_eq!(presets.len(), 5, "expected 5 builtin presets");
    assert!(presets.iter().any(|p| p.id == "deepseek"));
    assert!(presets.iter().any(|p| p.id == "ollama" && p.is_local && !p.requires_key));
}

#[test]
fn set_llm_config_fills_from_preset() {
    let state = AppState::in_memory(unique_ws("preset-fill")).expect("state");

    // Ollama preset: no key, base_url/model omitted -> filled from the preset.
    state
        .set_llm_config_with(Some("ollama".into()), String::new(), String::new(), String::new())
        .expect("ollama preset");
    let status = state.llm_config_status();
    assert_eq!(status.provider_id, "ollama");
    assert_eq!(status.base_url, "http://localhost:11434/v1");
    assert_eq!(status.model, "qwen2.5-coder");

    // custom without base_url/model is rejected.
    assert!(state
        .set_llm_config_with(Some("custom".into()), String::new(), String::new(), String::new())
        .is_err());
}
