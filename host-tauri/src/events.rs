//! The Tauri transport for the one event envelope.
//!
//! The envelope itself, the event names and the [`EventSink`] trait live in
//! `host-core`; this module adds the transport the webview needs and re-exports
//! the rest, so `host_tauri::events::…` keeps resolving for every consumer.

pub use host_core::events::*;

use serde_json::Value;
use std::sync::Arc;

/// Delivers events to the Tauri webview, wrapped in the envelope.
///
/// The webview unwraps at its single boundary (`ui/src/api/tauri.ts`'s
/// `onHostEvent`), so panel callbacks go on reading the payload they always read.
pub struct TauriEventSink {
    app: tauri::AppHandle,
    agent_id: String,
    /// The task whose events this sink carries, when it is a run's (v1.0 M6-3a).
    task_id: Option<String>,
}

impl TauriEventSink {
    /// `agent_id` is the identity every envelope this sink sends carries
    /// (`AppState::agent_id`).
    pub fn new(app: tauri::AppHandle, agent_id: impl Into<String>) -> Self {
        Self {
            app,
            agent_id: agent_id.into(),
            task_id: None,
        }
    }

    /// The same sink, stamping every envelope with `task_id` (v1.0 M6-3a).
    ///
    /// Bound at construction, which is when the caller knows which task it is serving; a sink
    /// built without it keeps publishing `task_id: null`.
    pub fn for_task(mut self, task_id: Option<String>) -> Self {
        self.task_id = task_id;
        self
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: Value) {
        use tauri::Emitter;
        // The struct, not a `Value`: Tauri serialises it field by field, so
        // `version` stays the first key of the object.
        let _ = self.app.emit(
            event,
            envelope(
                kind::EVENT,
                Some(event),
                &self.agent_id,
                self.task_id.as_deref(),
                payload,
            ),
        );
    }

    fn with_task(&self, task_id: Option<&str>) -> Option<Arc<dyn EventSink>> {
        Some(Arc::new(Self {
            app: self.app.clone(),
            agent_id: self.agent_id.clone(),
            task_id: task_id.map(str::to_string),
        }))
    }
}
