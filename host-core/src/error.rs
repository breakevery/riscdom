//! Host error type.
//!
//! `Display` output is what the frontend sees through Tauri commands, so it
//! must never contain the API key or other secrets.

use thiserror::Error;

/// Errors produced by the host layer.
#[derive(Debug, Error)]
pub enum HostError {
    #[error("not configured: {0}")]
    NotConfigured(String),

    #[error("policy denied: {0}")]
    Policy(String),

    #[error("agent error: {0}")]
    Agent(#[from] agent::AgentError),

    #[error("audit error: {0}")]
    Audit(#[from] audit::AuditError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl HostError {
    /// A message safe to hand to the frontend (currently just `Display`).
    pub fn user_message(&self) -> String {
        self.to_string()
    }
}
