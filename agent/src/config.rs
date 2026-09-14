//! Agent configuration, loaded from environment variables.

use crate::error::AgentError;
use crate::presets::{ProviderPreset, DEFAULT_PRESET_ID};
use std::fmt;

/// Runtime configuration for the agent.
#[derive(Clone)]
pub struct AgentConfig {
    /// API key (from `DEEPSEEK_API_KEY`). Never logged or audited.
    pub api_key: String,
    /// API base URL (default `https://api.deepseek.com`).
    pub base_url: String,
    /// Model name (default `deepseek-chat`).
    pub model: String,
    /// Provider preset id (default [`DEFAULT_PRESET_ID`]).
    pub provider_id: String,
    /// Max tool-use iterations per run.
    pub max_iterations: u32,
    /// Per-request HTTP timeout.
    pub request_timeout_secs: u64,
}

impl AgentConfig {
    /// Load configuration from the environment.
    ///
    /// Required: `DEEPSEEK_API_KEY`.
    /// Optional: `DEEPSEEK_BASE_URL`, `DEEPSEEK_MODEL`, `RISCDOM_MAX_ITERATIONS`,
    /// `RISCDOM_REQUEST_TIMEOUT`.
    pub fn from_env() -> Result<Self, AgentError> {
        let api_key = std::env::var("DEEPSEEK_API_KEY")
            .map_err(|_| AgentError::Config("DEEPSEEK_API_KEY is not set".into()))?;
        if api_key.trim().is_empty() {
            return Err(AgentError::Config("DEEPSEEK_API_KEY is empty".into()));
        }

        let base_url = std::env::var("DEEPSEEK_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com".to_string());
        let model =
            std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string());
        let max_iterations = std::env::var("RISCDOM_MAX_ITERATIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);
        let request_timeout_secs = std::env::var("RISCDOM_REQUEST_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(120);

        Ok(Self {
            api_key,
            base_url,
            model,
            provider_id: DEFAULT_PRESET_ID.to_string(),
            max_iterations,
            request_timeout_secs,
        })
    }

    /// Full chat-completions endpoint URL.
    pub fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// A preset template for DeepSeek (the default provider).
    ///
    /// `api_key` is empty; callers must fill it in (or call [`Self::validate`] to
    /// discover that it is required).
    pub fn deepseek_default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.deepseek.com".to_string(),
            model: "deepseek-chat".to_string(),
            provider_id: DEFAULT_PRESET_ID.to_string(),
            max_iterations: 10,
            request_timeout_secs: 120,
        }
    }

    /// Build a configuration from a provider preset.
    ///
    /// Fails loudly (never silently) when a key-requiring provider is selected
    /// without a key, and runs [`Self::validate`] as a final check.
    pub fn from_preset(
        preset: &ProviderPreset,
        api_key: Option<String>,
    ) -> Result<Self, AgentError> {
        let api_key = api_key.unwrap_or_default();
        if preset.requires_key && api_key.trim().is_empty() {
            return Err(AgentError::Config(format!(
                "provider '{}' requires an api_key",
                preset.id
            )));
        }
        let config = Self {
            api_key,
            base_url: preset.base_url.clone(),
            model: preset.default_model.clone(),
            provider_id: preset.id.clone(),
            max_iterations: 10,
            request_timeout_secs: 120,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration.
    ///
    /// - `base_url` must start with `http://` or `https://`
    /// - `model` must be non-empty
    /// - local endpoints (localhost / 127.0.0.1 / ::1) may omit `api_key`
    /// - non-local endpoints require a non-empty `api_key`
    pub fn validate(&self) -> Result<(), AgentError> {
        let base = self.base_url.trim();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(AgentError::Config(format!(
                "base_url must start with http:// or https:// (got {base:?})"
            )));
        }
        if self.model.trim().is_empty() {
            return Err(AgentError::Config("model must not be empty".into()));
        }
        if !is_local_url(base) && self.api_key.trim().is_empty() {
            return Err(AgentError::Config(
                "api_key is required for non-local base_url".into(),
            ));
        }
        Ok(())
    }

    /// A masked form of the API key: first 4 + `****` + last 4.
    pub fn masked_key(&self) -> String {
        mask_key(&self.api_key)
    }
}

/// Mask a secret: first 4 + `****` + last 4. Short secrets are fully masked.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "*".repeat(chars.len().max(1));
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}****{tail}")
}

/// Is `url` a localhost endpoint (no API key required)?
fn is_local_url(url: &str) -> bool {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let hostport = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = hostport.rsplit_once(':').map(|(h, _)| h).unwrap_or(hostport);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
}

impl fmt::Debug for AgentConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentConfig")
            .field("api_key", &self.masked_key())
            .field("provider_id", &self.provider_id)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("max_iterations", &self.max_iterations)
            .field("request_timeout_secs", &self.request_timeout_secs)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AgentConfig {
        AgentConfig {
            api_key: "test-key-not-real".into(),
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-chat".into(),
            provider_id: "deepseek".into(),
            max_iterations: 10,
            request_timeout_secs: 120,
        }
    }

    #[test]
    fn debug_masks_api_key() {
        let cfg = sample();
        let text = format!("{cfg:?}");
        assert!(!text.contains("test-key-not-real"), "leaked key: {text}");
        assert!(text.contains("test****real"), "unexpected mask: {text}");
    }

    #[test]
    fn mask_key_shape() {
        assert_eq!(mask_key("test-key-not-real"), "test****real");
        assert_eq!(mask_key("short"), "*****");
        assert_eq!(mask_key(""), "*");
    }

    #[test]
    fn endpoint_joins_correctly() {
        assert_eq!(
            sample().endpoint(),
            "https://api.deepseek.com/chat/completions"
        );
        let mut cfg = sample();
        cfg.base_url = "https://example.com/".into();
        assert_eq!(cfg.endpoint(), "https://example.com/chat/completions");
    }

    #[test]
    fn from_env_requires_api_key() {
        // Only this test touches DEEPSEEK_API_KEY, so there is no parallel race.
        let saved = std::env::var("DEEPSEEK_API_KEY").ok();
        std::env::remove_var("DEEPSEEK_API_KEY");

        let err = AgentConfig::from_env().expect_err("should fail without key");
        assert!(matches!(err, AgentError::Config(_)));

        if let Some(v) = saved {
            std::env::set_var("DEEPSEEK_API_KEY", v);
        }
    }
}
