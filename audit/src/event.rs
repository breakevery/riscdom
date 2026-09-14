//! Audit event types.

use serde::{Deserialize, Serialize};

/// A single audit event (what the caller submits).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Milliseconds since the Unix epoch.
    pub timestamp_ms: i64,
    /// Who caused the event: `"sandbox"` / `"agent"` / `"host"` / `"human"`.
    pub actor: String,
    /// What happened, e.g. `"vm.start"`, `"llm.request"`.
    pub action: String,
    /// Free-form structured detail. Must never contain secrets (API keys, tokens).
    pub detail: serde_json::Value,
}

impl AuditEvent {
    /// Convenience constructor.
    pub fn new(
        actor: impl Into<String>,
        action: impl Into<String>,
        detail: serde_json::Value,
    ) -> Self {
        Self {
            timestamp_ms: now_ms(),
            actor: actor.into(),
            action: action.into(),
            detail,
        }
    }
}

/// An event as persisted, including its position in the hash chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: i64,
    pub event: AuditEvent,
    pub prev_hash: String,
    pub hash: String,
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
