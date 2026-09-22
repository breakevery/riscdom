//! The Tauri transport for the one event envelope.
//!
//! The envelope itself, the event names and the [`EventSink`] trait live in
//! `host-core`; this module adds the transport the webview needs and re-exports
//! the rest, so `host_tauri::events::…` keeps resolving for every consumer.

pub use host_core::events::*;

use serde_json::Value;

/// Delivers events to the Tauri webview, wrapped in the envelope.
///
/// The webview unwraps at its single boundary (`ui/src/api/tauri.ts`'s
/// `onHostEvent`), so panel callbacks go on reading the payload they always read.
pub struct TauriEventSink {
    app: tauri::AppHandle,
    agent_id: String,
}

impl TauriEventSink {
    /// `agent_id` is the identity every envelope this sink sends carries
    /// (`AppState::agent_id`).
    pub fn new(app: tauri::AppHandle, agent_id: impl Into<String>) -> Self {
        Self {
            app,
            agent_id: agent_id.into(),
        }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: Value) {
        use tauri::Emitter;
        // The struct, not a `Value`: Tauri serialises it field by field, so
        // `version` stays the first key of the object.
        let _ = self
            .app
            .emit(event, event_envelope(event, &self.agent_id, payload));
    }
}
