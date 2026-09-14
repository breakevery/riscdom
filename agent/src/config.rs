//! Agent configuration, loaded from environment variables.

use crate::error::AgentError;
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
            max_iterations,
            request_timeout_secs,
        })
    }

    /// Full chat-completions endpoint URL.
    pub fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
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

impl fmt::Debug for AgentConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentConfig")
            .field("api_key", &self.masked_key())
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
            api_key: "sk-1234567890abcdef".into(),
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-chat".into(),
            max_iterations: 10,
            request_timeout_secs: 120,
        }
    }

    #[test]
    fn debug_masks_api_key() {
        let cfg = sample();
        let text = format!("{cfg:?}");
        assert!(!text.contains("sk-1234567890abcdef"), "leaked key: {text}");
        assert!(text.contains("sk-1****cdef"), "unexpected mask: {text}");
    }

    #[test]
    fn mask_key_shape() {
        assert_eq!(mask_key("sk-1234567890abcdef"), "sk-1****cdef");
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
