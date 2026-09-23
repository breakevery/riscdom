//! Event names emitted by the host, the envelope every event travels in, and the
//! [`EventSink`] abstraction so the same code runs under Tauri, under the control
//! plane and under tests.
//!
//! # The envelope
//!
//! Every event the host emits reaches a transport wrapped in one envelope, the
//! shape settled in `docs/control-plane-events.md` §2:
//!
//! ```json
//! { "version": 1, "kind": "event", "event": "agent:tool_call",
//!   "agent_id": "local-12345-1", "task_id": null, "ts": 1758533001207,
//!   "payload": { "name": "write_source", "arguments": {} } }
//! ```
//!
//! **The sink builds the envelope, not the emit site.** [`EventSink::emit`] keeps
//! its `(&str, Value)` signature, so no emit site and no implementation outside
//! this crate had to change shape; a sink that knows its own identity wraps, and
//! [`RecordingEventSink`] goes on recording the raw `(event, payload)` pair a test
//! asked about. The envelope is written with a struct — never through
//! `serde_json::Value` — because a `Value` map sorts its keys and would move
//! `version` away from the front of the object.

use serde::Serialize;
use serde_json::Value;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// One LLM iteration started (maps to an audited `agent.llm.request`).
pub const EV_AGENT_ITERATION: &str = "agent:iteration";
/// The model requested a tool call (`agent.tool.call`).
pub const EV_AGENT_TOOL_CALL: &str = "agent:tool_call";
/// A tool call returned (`agent.tool.result`).
pub const EV_AGENT_TOOL_RESULT: &str = "agent:tool_result";
/// The run finished (emitted directly by `run_agent`).
pub const EV_AGENT_FINAL: &str = "agent:final";
/// New serial output (incremental), pushed by the sandbox via the agent's
/// serial observer (`AgentLoop::subscribe_serial`).
pub const EV_SERIAL_CHUNK: &str = "serial:chunk";
/// Download progress for the pinned RISC-V toolchain (v0.9: shared shape with
/// [`EV_QEMU_DOWNLOAD`]).
///
/// The payload is the internally tagged `DownloadEvent` enum under the tag `state`:
/// `started` / `progress` / `verifying` / `extracting` / `done` / `failed` /
/// `cancelled`.
pub const TOOLCHAIN_DOWNLOAD: &str = "toolchain:download";

/// Download progress for the pinned QEMU build (v0.9 sandbox F1).
///
/// The same payload shape as [`TOOLCHAIN_DOWNLOAD`]: the two assemblies report
/// themselves the same way, so a client reads one vocabulary for both. Today no QEMU
/// release is pinned (`docs/qemu-distribution.md` §5), so the only thing this family
/// carries is the refusal.
pub const EV_QEMU_DOWNLOAD: &str = "qemu:download";
/// Incremental assistant text from the LLM stream (`AgentLoop::subscribe_stream`).
pub const EV_AGENT_STREAM_DELTA: &str = "agent:stream:delta";
/// The LLM stream finished.
pub const EV_AGENT_STREAM_DONE: &str = "agent:stream:done";
/// VM lifecycle change (start/stop/snapshot).
pub const EV_VM_STATE: &str = "vm:state";
/// Environment preflight progress (v0.4 batch 3).
pub const EV_PREFLIGHT: &str = "preflight:progress";
/// An audit write failed after its retries (v0.8).
///
/// Always sent, whatever the alert setting says: the alert (banner + popup) is
/// what can be switched off, the event and its log line cannot.
pub const EV_AUDIT_FAILED: &str = "audit:failed";

/// A sandbox switch finished — with a new sandbox running, or with a reason it
/// did not (v0.9 sandbox F2b-2).
///
/// The payload is [`sandbox_switch_payload`]: `{from, to, ok, reason}`, where
/// `from` is the definition that was current before (or `null` when none was) and
/// `to` is the definition the switch was asked for. One event per attempt, either
/// way: a client that sees `ok: false` reads `reason` for the code.
pub const EV_SANDBOX_SWITCH: &str = "sandbox:switch";

/// A sandbox **request** changed state (v0.9 sandbox F2c): one at `pending` when
/// an actor that may not switch leaves an ask behind, then one at `approved` or
/// `rejected` when another actor decides it.
///
/// The payload is [`sandbox_request_payload`]: `{id, status, requester, action}`.
/// `status` is one of the four names of the request's own enum (`expired` is
/// reserved — v0.9 sets no TTL), and approving does **not** perform the change:
/// the switch is a second, authorised call.
pub const EV_SANDBOX_REQUEST: &str = "sandbox:request";

/// The envelope schema version (v0.9 line). A payload field added later does not
/// bump it; a change to a field's meaning, type, or presence does.
pub const ENVELOPE_VERSION: u32 = 1;

/// The frame kinds of the event stream.
pub mod kind {
    /// One of the thirteen events; `event` names it.
    pub const EVENT: &str = "event";
    /// The stream opened; the payload describes the buffer and the filters.
    pub const HELLO: &str = "hello";
    /// A replay request was too old to fill. **Not implemented yet**; the name is
    /// reserved so it does not change when it lands.
    pub const GAP: &str = "gap";
}

/// Epoch milliseconds — the timestamp unit of every event and every frame.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One event, wrapped for the wire.
///
/// Field order is the wire order: `version` is the first key a client sees. Build
/// one through [`envelope`] rather than by hand, so every transport agrees.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Envelope {
    /// Envelope schema version ([`ENVELOPE_VERSION`]).
    pub version: u32,
    /// [`kind::EVENT`] / [`kind::HELLO`] / [`kind::GAP`].
    pub kind: String,
    /// One of the thirteen event names, or `null` for `hello` / `gap`.
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
    /// The wire form: a single line, fields in declaration order.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "null".to_string())
    }

    /// The same envelope as a `serde_json::Value`.
    ///
    /// For a consumer that needs a value rather than bytes, and accepts that a
    /// value map has no key order. Transports use [`Envelope::to_json`].
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Wrap anything in an envelope. The one place the shape is written down.
pub fn envelope(
    kind: &str,
    event: Option<&str>,
    agent_id: &str,
    task_id: Option<&str>,
    payload: Value,
) -> Envelope {
    Envelope {
        version: ENVELOPE_VERSION,
        kind: kind.to_string(),
        event: event.map(str::to_string),
        agent_id: agent_id.to_string(),
        task_id: task_id.map(str::to_string),
        ts: now_ms(),
        payload,
    }
}

/// One of the thirteen events, wrapped. The common case: no task identity yet, so
/// `task_id` is `null`.
pub fn event_envelope(event: &str, agent_id: &str, payload: Value) -> Envelope {
    envelope(kind::EVENT, Some(event), agent_id, None, payload)
}

/// The `vm:state` payload.
///
/// `name` is **always present** — `null` for a start/stop, the snapshot's name for
/// a save (`docs/control-plane-events.md` §3.1). A client tests
/// `payload.state == "snapshot"`, never "does `name` exist".
pub fn vm_state_payload(
    state: &str,
    running: bool,
    since_ms: Option<i64>,
    name: Option<&str>,
) -> Value {
    serde_json::json!({
        "state": state,
        "running": running,
        "since_ms": since_ms,
        "name": name,
    })
}

/// The `sandbox:switch` payload.
///
/// `from` is the definition that was current before the switch (`null` when the
/// node had none), `to` is the one it was asked for, `ok` says whether it is
/// running now, and `reason` carries the code when it is not — the same names the
/// API's error model uses, so a client reads one vocabulary for both surfaces.
pub fn sandbox_switch_payload(
    from: Option<&str>,
    to: &str,
    ok: bool,
    reason: Option<&str>,
) -> Value {
    serde_json::json!({
        "from": from,
        "to": to,
        "ok": ok,
        "reason": reason,
    })
}

/// The `sandbox:request` payload.
///
/// `requester` is the actor that left the ask (an agent id, or the interface's)
/// and `action` is what it wants — the same vocabulary the API's request body
/// uses, so a client reads one set of names for both surfaces.
pub fn sandbox_request_payload(id: &str, status: &str, requester: &str, action: &str) -> Value {
    serde_json::json!({
        "id": id,
        "status": status,
        "requester": requester,
        "action": action,
    })
}

/// The `audit:failed` payload. The key is `message`, matching the API error model.
pub fn audit_failed_payload(message: &str) -> Value {
    serde_json::json!({ "message": message })
}

/// Anything that can deliver an event to the frontend.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

/// Collects events in memory (tests).
///
/// Records the `(event, payload)` pair exactly as the kernel emitted it — the raw
/// payload, not the envelope — because that is what a host test asserts about.
/// The transports above are where the envelope is applied.
#[derive(Default)]
pub struct RecordingEventSink {
    events: Mutex<Vec<(String, Value)>>,
}

impl RecordingEventSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// All recorded `(event, payload)` pairs.
    pub fn events(&self) -> Vec<(String, Value)> {
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
    fn emit(&self, event: &str, payload: Value) {
        if let Ok(mut g) = self.events.lock() {
            g.push((event.to_string(), payload));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The envelope every transport puts around the thirteen events.
    fn wrap(event: &str, payload: Value) -> Envelope {
        event_envelope(event, "local-4711-1", payload)
    }

    fn assert_envelope(env: &Envelope, event: &str) {
        assert_eq!(env.version, ENVELOPE_VERSION, "version");
        assert_eq!(env.kind, kind::EVENT, "kind");
        assert_eq!(env.event.as_deref(), Some(event), "event");
        assert_eq!(env.agent_id, "local-4711-1", "agent_id");
        assert_eq!(env.task_id, None, "task_id");
        assert!(env.ts > 0, "ts");
        assert!(env.payload.is_object(), "payload");
    }

    #[test]
    fn the_envelope_puts_version_first_on_the_wire() {
        let json = wrap(EV_AGENT_FINAL, serde_json::json!({ "kind": "final" })).to_json();
        assert!(
            json.starts_with("{\"version\":1,\"kind\":\"event\","),
            "{json}"
        );
        assert!(json.contains("\"task_id\":null"), "{json}");
    }

    #[test]
    fn agent_iteration_keeps_its_payload() {
        let env = wrap(
            EV_AGENT_ITERATION,
            serde_json::json!({ "model": "local-model", "messages": [] }),
        );
        assert_envelope(&env, EV_AGENT_ITERATION);
        assert!(env.payload["model"].is_string());
        assert!(env.payload["messages"].is_array());
    }

    #[test]
    fn agent_tool_call_keeps_its_payload() {
        let env = wrap(
            EV_AGENT_TOOL_CALL,
            serde_json::json!({ "name": "write_source", "arguments": { "path": "src/main.c" } }),
        );
        assert_envelope(&env, EV_AGENT_TOOL_CALL);
        assert_eq!(env.payload["name"], "write_source");
        assert!(env.payload["arguments"].is_object());
    }

    #[test]
    fn agent_tool_result_keeps_its_payload() {
        let env = wrap(
            EV_AGENT_TOOL_RESULT,
            serde_json::json!({ "ok": true, "result": "wrote 24 bytes" }),
        );
        assert_envelope(&env, EV_AGENT_TOOL_RESULT);
        assert_eq!(env.payload["ok"], true);
        assert!(env.payload["result"].is_string());
    }

    #[test]
    fn agent_final_keeps_its_payload() {
        let env = wrap(
            EV_AGENT_FINAL,
            serde_json::json!({ "kind": "final", "content": "done", "reason": null, "iterations": 1 }),
        );
        assert_envelope(&env, EV_AGENT_FINAL);
        assert_eq!(env.payload["kind"], "final");
        assert_eq!(env.payload["iterations"], 1);
    }

    #[test]
    fn agent_stream_delta_keeps_its_payload() {
        let env = wrap(
            EV_AGENT_STREAM_DELTA,
            serde_json::json!({ "text": "Compiling" }),
        );
        assert_envelope(&env, EV_AGENT_STREAM_DELTA);
        assert_eq!(env.payload["text"], "Compiling");
    }

    #[test]
    fn agent_stream_done_keeps_an_empty_payload() {
        let env = wrap(EV_AGENT_STREAM_DONE, serde_json::json!({}));
        assert_envelope(&env, EV_AGENT_STREAM_DONE);
        assert_eq!(env.payload.as_object().map(|o| o.len()), Some(0));
    }

    #[test]
    fn serial_chunk_keeps_its_payload() {
        let env = wrap(
            EV_SERIAL_CHUNK,
            serde_json::json!({ "chunk": "hello from riscv\n" }),
        );
        assert_envelope(&env, EV_SERIAL_CHUNK);
        assert_eq!(env.payload["chunk"], "hello from riscv\n");
    }

    #[test]
    fn preflight_progress_keeps_its_payload() {
        let env = wrap(
            EV_PREFLIGHT,
            serde_json::json!({ "step": "gcc_runs", "state": "ok", "detail": "gcc 13.2.0" }),
        );
        assert_envelope(&env, EV_PREFLIGHT);
        assert_eq!(env.payload["step"], "gcc_runs");
        assert_eq!(env.payload["state"], "ok");
    }

    #[test]
    fn vm_state_carries_a_name_on_every_variant() {
        // v0.9 change: `name` is always present, `null` when it is not a snapshot.
        let running = vm_state_payload("running", true, Some(1758533002050), None);
        assert_eq!(running["state"], "running");
        assert_eq!(running["running"], true);
        assert_eq!(running["since_ms"], 1758533002050i64);
        assert!(running.get("name").is_some(), "name must exist");
        assert!(running["name"].is_null());

        let snapshot = vm_state_payload("snapshot", true, Some(1), Some("after-blink"));
        assert_eq!(snapshot["name"], "after-blink");

        let env = wrap(EV_VM_STATE, running);
        assert_envelope(&env, EV_VM_STATE);
        assert!(env.payload.get("name").is_some());
    }

    #[test]
    fn audit_failed_uses_the_message_key() {
        // v0.9 change: `error` became `message`, matching the API error model.
        let env = wrap(EV_AUDIT_FAILED, audit_failed_payload("database is locked"));
        assert_envelope(&env, EV_AUDIT_FAILED);
        assert_eq!(env.payload["message"], "database is locked");
        assert!(env.payload.get("error").is_none(), "the old key is gone");
    }

    #[test]
    fn toolchain_download_payload_is_flat_and_tagged_state() {
        // v0.9 change: the tag is `state`, and the variant fields sit beside it.
        let payload = serde_json::to_value(crate::toolchain_download::DownloadEvent::Progress {
            downloaded: 10,
            total: Some(20),
        })
        .expect("serialises");
        assert_eq!(payload["state"], "progress");
        assert_eq!(payload["downloaded"], 10);
        assert_eq!(payload["total"], 20);
        assert!(payload.get("kind").is_none(), "the old tag is gone");

        let env = wrap(TOOLCHAIN_DOWNLOAD, payload);
        assert_envelope(&env, TOOLCHAIN_DOWNLOAD);
        assert_eq!(env.payload["state"], "progress");
    }

    #[test]
    fn all_events_are_named() {
        // A guard against a name drifting: the doc lists fourteen. The list was
        // eleven while `qemu:download` (F1) was missing from it, and thirteen
        // before `sandbox:request` (F2c) — a guard that misses an event is not a
        // guard (v0.9 sandbox F2b-2, F2c).
        let names = [
            EV_AGENT_ITERATION,
            EV_AGENT_TOOL_CALL,
            EV_AGENT_TOOL_RESULT,
            EV_AGENT_FINAL,
            EV_AGENT_STREAM_DELTA,
            EV_AGENT_STREAM_DONE,
            EV_SERIAL_CHUNK,
            EV_VM_STATE,
            EV_PREFLIGHT,
            EV_AUDIT_FAILED,
            TOOLCHAIN_DOWNLOAD,
            EV_QEMU_DOWNLOAD,
            EV_SANDBOX_SWITCH,
            EV_SANDBOX_REQUEST,
        ];
        assert_eq!(names.len(), 14);
        // Every name is unique, so a copy-paste cannot hide a missing one.
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "{names:?}");
    }

    #[test]
    fn sandbox_switch_names_both_ends() {
        let payload = sandbox_switch_payload(
            Some("blink"),
            "scratch",
            false,
            Some("sandbox_qemu_missing: not a file"),
        );
        assert_eq!(payload["from"], "blink");
        assert_eq!(payload["to"], "scratch");
        assert_eq!(payload["ok"], false);
        assert!(payload["reason"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sandbox_qemu_missing"));

        // A node with nothing current says so with `null`, not with an empty string.
        let fresh = sandbox_switch_payload(None, "blink", true, None);
        assert!(fresh["from"].is_null());
        assert_eq!(fresh["ok"], true);
        assert!(fresh["reason"].is_null());

        let env = wrap(EV_SANDBOX_SWITCH, fresh);
        assert_envelope(&env, EV_SANDBOX_SWITCH);
    }

    #[test]
    fn the_request_payload_names_the_ask_and_its_state() {
        let pending = sandbox_request_payload("req-9-3", "pending", "local-7-1", "switch");
        assert_eq!(pending["id"], "req-9-3");
        assert_eq!(pending["status"], "pending");
        assert_eq!(pending["requester"], "local-7-1");
        assert_eq!(pending["action"], "switch");
        // Four keys, no more: a client branches on names, and the meanings do not
        // change with the state (an `approved` frame carries the same four).
        assert_eq!(pending.as_object().map(|o| o.len()), Some(4));

        let env = wrap(EV_SANDBOX_REQUEST, pending);
        assert_envelope(&env, EV_SANDBOX_REQUEST);
        assert_eq!(EV_SANDBOX_REQUEST, "sandbox:request");
    }

    #[test]
    fn the_recording_sink_still_records_the_raw_payload() {
        let sink = RecordingEventSink::new();
        sink.emit(EV_SERIAL_CHUNK, serde_json::json!({ "chunk": "a" }));
        sink.emit(EV_SERIAL_CHUNK, serde_json::json!({ "chunk": "b" }));
        assert_eq!(sink.count(EV_SERIAL_CHUNK), 2);
        assert_eq!(sink.serial_text(), "ab");
        // The raw payload, not an envelope.
        let (_, payload) = sink.events().remove(0);
        assert!(payload.get("chunk").is_some());
        assert!(payload.get("version").is_none());
    }
}
