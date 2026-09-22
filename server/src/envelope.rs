//! The event envelope every SSE frame carries
//! (`docs/control-plane-events.md` §2).
//!
//! One envelope for all events. This batch wraps the host's payloads exactly as
//! they are emitted; normalising them to the documented v1 shapes is a later
//! batch (the emit sites are untouched here).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// The envelope schema version. `1` for the whole v0.9 line.
pub const ENVELOPE_VERSION: u32 = 1;

/// The `kind` of a frame.
pub mod kind {
    /// One of the eleven events; `event` names it.
    pub const EVENT: &str = "event";
    /// The stream opened; `payload` describes the buffer and the filters.
    pub const HELLO: &str = "hello";
    /// A replay request was too old to fill. **Not implemented in this batch**;
    /// reserved so the name does not change later.
    pub const GAP: &str = "gap";
}

/// One event, wrapped for the wire.
///
/// Field order is the wire order: `version` is the first key a client sees, which
/// is what `docs/control-plane-events.md` promises.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    /// Envelope schema version (`1`).
    pub version: u32,
    /// `event` / `hello` / `gap`.
    pub kind: String,
    /// One of the eleven event names, or `null` for `hello` / `gap`.
    pub event: Option<String>,
    /// The agent that caused the event, `<device>-<pid>-<seq>`.
    pub agent_id: String,
    /// The dispatched task it belongs to, or `null` when not tied to one.
    pub task_id: Option<String>,
    /// Epoch milliseconds.
    pub ts: i64,
    /// Event-specific body.
    pub payload: Value,
}

impl Envelope {
    /// The frame a client receives first on `/v0/events`.
    pub fn hello(agent_id: impl Into<String>, ts: i64, buffer: Value, filters: Value) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            kind: kind::HELLO.to_string(),
            event: None,
            agent_id: agent_id.into(),
            task_id: None,
            ts,
            payload: serde_json::json!({ "buffer": buffer, "filters": filters }),
        }
    }

    /// One of the eleven events.
    pub fn event(
        event: impl Into<String>,
        agent_id: impl Into<String>,
        task_id: Option<String>,
        ts: i64,
        payload: Value,
    ) -> Self {
        Self {
            version: ENVELOPE_VERSION,
            kind: kind::EVENT.to_string(),
            event: Some(event.into()),
            agent_id: agent_id.into(),
            task_id,
            ts,
            payload,
        }
    }

    /// Serialise for the `data:` line. `serde_json` writes one line, so an
    /// envelope can never break a frame in half.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "null".to_string())
    }
}

/// Epoch milliseconds, the timestamp unit of every frame.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_is_the_documented_shape() {
        let env = Envelope::hello(
            "dev-1-1",
            1234,
            serde_json::json!({ "from": 0, "to": 0 }),
            serde_json::json!({ "event": [], "agent_id": null, "task_id": null }),
        );
        assert_eq!(env.version, 1);
        assert_eq!(env.kind, "hello");
        assert_eq!(env.event, None);
        assert_eq!(env.agent_id, "dev-1-1");
        assert_eq!(env.payload["buffer"]["from"], 0);
        assert!(env.payload["filters"]["event"].is_array());
    }

    #[test]
    fn version_is_the_first_key_on_the_wire() {
        let env = Envelope::event(
            "agent:tool_call",
            "dev-1-1",
            Some("task-1-1".into()),
            7,
            serde_json::json!({ "name": "write_source" }),
        );
        let json = env.to_json();
        assert!(json.starts_with("{\"version\":1,"), "got {json}");
        assert!(json.contains("\"kind\":\"event\""));
        assert!(json.contains("\"task_id\":\"task-1-1\""));
    }

    #[test]
    fn a_detached_event_has_a_null_task_id() {
        let env = Envelope::event("vm:state", "dev-1-1", None, 7, serde_json::json!({}));
        assert!(env.to_json().contains("\"task_id\":null"));
    }
}
