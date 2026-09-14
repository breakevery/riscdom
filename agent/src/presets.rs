//! Built-in LLM provider presets.
//!
//! Presets are **pure data**: they describe how to reach an OpenAI-compatible
//! endpoint. No provider-specific logic lives anywhere else.
//!
//! DeepSeek is the default ([`DEFAULT_PRESET_ID`]).

use serde::{Deserialize, Serialize};

/// A selectable LLM provider preset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderPreset {
    /// Stable id, e.g. `"deepseek"` / `"custom"`.
    pub id: String,
    /// Human-readable name for the UI.
    pub display_name: String,
    /// API base URL. Empty means "the user must fill this in" (custom).
    pub base_url: String,
    /// Suggested model name. Empty means "the user must fill this in".
    pub default_model: String,
    /// Whether an API key is required.
    pub requires_key: bool,
    /// Whether the endpoint is a local server (no network / no key needed).
    pub is_local: bool,
}

/// The default preset id (DeepSeek).
pub const DEFAULT_PRESET_ID: &str = "deepseek";

/// The built-in provider presets.
pub fn builtin_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "deepseek".into(),
            display_name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com".into(),
            default_model: "deepseek-chat".into(),
            requires_key: true,
            is_local: false,
        },
        ProviderPreset {
            id: "openai".into(),
            display_name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            default_model: "gpt-4o-mini".into(),
            requires_key: true,
            is_local: false,
        },
        ProviderPreset {
            id: "ollama".into(),
            display_name: "Ollama（本地）".into(),
            base_url: "http://localhost:11434/v1".into(),
            default_model: "qwen2.5-coder".into(),
            requires_key: false,
            is_local: true,
        },
        ProviderPreset {
            id: "lmstudio".into(),
            display_name: "LM Studio（本地）".into(),
            base_url: "http://localhost:1234/v1".into(),
            default_model: String::new(),
            requires_key: false,
            is_local: true,
        },
        ProviderPreset {
            id: "custom".into(),
            display_name: "自定义".into(),
            base_url: String::new(),
            default_model: String::new(),
            // The user declares whether their custom endpoint needs a key; we
            // default to "no" and let `AgentConfig::validate` decide based on
            // whether the URL is local.
            requires_key: false,
            is_local: false,
        },
    ]
}

/// Look up a preset by id.
pub fn find_preset(id: &str) -> Option<ProviderPreset> {
    builtin_presets().into_iter().find(|p| p.id == id)
}
