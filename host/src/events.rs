//! Event names emitted by the host, plus an [`EventSink`] abstraction so the
//! same code runs under Tauri and under tests.

use std::sync::Mutex;

/// One LLM iteration started (maps to an audited `agent.llm.request`).
pub const EV_AGENT_ITERATION: &str = "agent:iteration";
/// The model requested a tool call (`agent.tool.call`).
pub const EV_AGENT_TOOL_CALL: &str = "agent:tool_call";
/// A tool call returned (`agent.tool.result`).
pub const EV_AGENT_TOOL_RESULT: &str = "agent:tool_result";
/// The run finished (emitted directly by `run_agent`).
pub const EV_AGENT_FINAL: &str = "agent:final";
/// New serial output (incremental).
pub const EV_SERIAL_CHUNK: &str = "serial:chunk";
/// VM lifecycle change (start/stop/snapshot).
pub const EV_VM_STATE: &str = "vm:state";

/// Anything that can deliver an event to the frontend.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// Delivers events to the Tauri webview.
pub struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        use tauri::Emitter;
        let _ = self.app.emit(event, payload);
    }
}

/// Collects events in memory (tests).
#[derive(Default)]
pub struct RecordingEventSink {
    events: Mutex<Vec<(String, serde_json::Value)>>,
}

impl RecordingEventSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// All recorded `(event, payload)` pairs.
    pub fn events(&self) -> Vec<(String, serde_json::Value)> {
        self.events.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Count of events with a given name.
    pub fn count(&self, event: &str) -> usize {
        self.events
            .lock()
            .map(|g| g.iter().filter(|(e, _)| e == event).count())
            .unwrap_or(0)
    }

    /// The concatenation of all `serial:chunk` payloads.
    pub fn serial_text(&self) -> String {
        let mut out = String::new();
        if let Ok(g) = self.events.lock() {
            for (e, p) in g.iter() {
                if e == EV_SERIAL_CHUNK {
                    if let Some(c) = p.get("chunk").and_then(|v| v.as_str()) {
                        out.push_str(c);
                    }
                }
            }
        }
        out
    }
}

impl EventSink for RecordingEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Ok(mut g) = self.events.lock() {
            g.push((event.to_string(), payload));
        }
    }
}
