//! Stage 11a — provider presets (pure data) and `AgentConfig::from_preset`.

use agent::presets::{builtin_presets, find_preset, ProviderPreset, DEFAULT_PRESET_ID};
use agent::{AgentConfig, AgentError};

#[test]
fn builtin_presets_shape() {
    let presets = builtin_presets();
    assert_eq!(presets.len(), 5, "expected 5 builtin presets");

    let mut ids: Vec<&str> = presets.iter().map(|p| p.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 5, "preset ids must be unique");

    assert!(
        presets.iter().any(|p| p.id == DEFAULT_PRESET_ID),
        "default preset id must exist"
    );
    // DeepSeek stays the default and requires a key.
    let ds = find_preset("deepseek").expect("deepseek preset");
    assert_eq!(ds.base_url, "https://api.deepseek.com");
    assert_eq!(ds.default_model, "deepseek-chat");
    assert!(ds.requires_key && !ds.is_local);
}

#[test]
fn find_preset_hits_and_misses() {
    assert!(find_preset("ollama").is_some());
    assert!(find_preset("nope").is_none());
}

#[test]
fn from_preset_requires_key_for_cloud() {
    let ds = find_preset("deepseek").unwrap();
    assert!(matches!(
        AgentConfig::from_preset(&ds, None),
        Err(AgentError::Config(_))
    ));
    assert!(AgentConfig::from_preset(&ds, Some("test-key".into())).is_ok());
}

#[test]
fn from_preset_allows_no_key_for_local() {
    let ollama = find_preset("ollama").unwrap();
    let cfg = AgentConfig::from_preset(&ollama, None).expect("ollama preset");
    assert_eq!(cfg.provider_id, "ollama");
    assert_eq!(cfg.base_url, "http://localhost:11434/v1");
    assert_eq!(cfg.model, "qwen2.5-coder");
    assert!(cfg.api_key.is_empty());
}

#[test]
fn from_preset_custom_with_filled_values_is_ok() {
    // A self-hosted local gateway: base_url + model given, no key needed.
    let custom = ProviderPreset {
        id: "custom".into(),
        display_name: "自定义".into(),
        base_url: "http://127.0.0.1:8000/v1".into(),
        default_model: "my-model".into(),
        requires_key: false,
        is_local: false,
    };
    let cfg = AgentConfig::from_preset(&custom, None).expect("custom local preset");
    assert_eq!(cfg.provider_id, "custom");
    assert_eq!(cfg.base_url, "http://127.0.0.1:8000/v1");
    assert_eq!(cfg.model, "my-model");
}

#[test]
fn from_preset_custom_remote_without_key_is_err() {
    // A remote custom endpoint still needs a key (validate() enforces it).
    let remote = ProviderPreset {
        id: "custom".into(),
        display_name: "自定义".into(),
        base_url: "https://my-gateway.example.com/v1".into(),
        default_model: "my-model".into(),
        requires_key: false,
        is_local: false,
    };
    assert!(AgentConfig::from_preset(&remote, None).is_err());
    assert!(AgentConfig::from_preset(&remote, Some("test-key".into())).is_ok());
}

#[test]
fn from_preset_custom_without_url_is_err() {
    let custom = find_preset("custom").unwrap();
    assert!(AgentConfig::from_preset(&custom, None).is_err());
}
