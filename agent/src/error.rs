//! Agent error type.
//!
//! NOTE: error messages must never include the API key. `AgentConfig`'s `Debug`
//! masks it, and errors here only carry status codes / body snippets.

use thiserror::Error;

/// Errors produced by the agent layer.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("config error: {0}")]
    Config(String),

    #[error("http error: {0}")]
    Http(String),

    #[error("api error: {0}")]
    Api(String),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("policy denied: {0}")]
    PolicyDenied(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("sandbox error: {0}")]
    Sandbox(#[from] sandbox::SandboxError),

    #[error("{0}")]
    Other(String),
}
