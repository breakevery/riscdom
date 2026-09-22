//! The SSE push side: the hub that fans events out, the frame format, and the
//! `host::EventSink` implementation that feeds it.
//!
//! The wire format is settled in `docs/control-plane-events.md` §1: a frame uses
//! `id:` and `data:` only, and ends with a blank line. There is deliberately no
//! `event:` field — that would break a browser's single `onmessage` handler, and
//! the envelope's `event` field is the router instead.

use crate::envelope::{now_ms, Envelope};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Default number of frames a slow subscriber may fall behind before it is told
/// it lagged. Bounded on purpose: an unbounded buffer is a memory leak with a
/// friendly name.
pub const CHANNEL_CAPACITY: usize = 1024;

/// One thing the stream can carry.
#[derive(Debug, Clone, PartialEq)]
pub enum SseFrame {
    /// A wrapped event (or the opening `hello`).
    Envelope(Envelope),
    /// A heartbeat: written as an SSE comment line, ignored by every client that
    /// understands the format.
    Comment(String),
}

/// Serialise a frame exactly as it appears on the wire.
///
/// `seq` is the per-connection ordinal that, with the frame's timestamp, forms
/// the opaque `id:` a client stores and sends back as `Last-Event-ID`.
pub fn frame_bytes(frame: &SseFrame, seq: u64) -> Vec<u8> {
    match frame {
        SseFrame::Envelope(env) => {
            format!("id: {}-{}\ndata: {}\n\n", env.ts, seq, env.to_json()).into_bytes()
        }
        SseFrame::Comment(text) => format!(": {text}\n\n").into_bytes(),
    }
}

/// The fan-out point: every SSE subscriber attaches here.
pub struct SseHub {
    tx: broadcast::Sender<SseFrame>,
}

impl SseHub {
    /// Create the hub. The returned handle is shared: it is the sink side and the
    /// subscribe side at once.
    pub fn new(capacity: usize) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(capacity);
        Arc::new(Self { tx })
    }

    /// Attach a subscriber.
    pub fn subscribe(&self) -> broadcast::Receiver<SseFrame> {
        self.tx.subscribe()
    }

    /// How many subscribers are attached right now (the `/v0/status` figure).
    pub fn subscribers(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Push a frame to every attached subscriber. Returns how many received it.
    pub fn publish(&self, frame: SseFrame) -> usize {
        self.tx.send(frame).unwrap_or(0)
    }

    /// Start the heartbeat on a **dedicated OS thread**.
    ///
    /// A tokio interval would need tokio's `time` feature, which this crate does
    /// not enable; a plain thread that sleeps and publishes needs nothing beyond
    /// the broadcast channel. It only publishes when someone is subscribed.
    pub fn start_heartbeat(self: &Arc<Self>, period: Duration, text: &'static str) {
        let hub = Arc::clone(self);
        std::thread::spawn(move || loop {
            std::thread::sleep(period);
            if hub.subscribers() == 0 {
                continue;
            }
            hub.publish(SseFrame::Comment(text.to_string()));
        });
    }
}

/// Brings the host's events to the stream.
///
/// This is the only place the two layers meet: it implements `host::EventSink`
/// (the kernel's emitter trait) by pushing each event into the hub. It holds no
/// Tauri handle and no `AppHandle` — it is passed to `AppState::run_agent` as the
/// `emitter` argument, so `host` needs no change to be driven from here.
pub struct HttpEventSink {
    hub: Arc<SseHub>,
    agent_id: String,
}

impl HttpEventSink {
    pub fn new(hub: Arc<SseHub>, agent_id: impl Into<String>) -> Self {
        Self {
            hub,
            agent_id: agent_id.into(),
        }
    }
}

impl host::EventSink for HttpEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let envelope = Envelope::event(event, self.agent_id.clone(), None, now_ms(), payload);
        self.hub.publish(SseFrame::Envelope(envelope));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::kind;
    use host::EventSink;

    fn text(frame: &SseFrame, seq: u64) -> String {
        String::from_utf8(frame_bytes(frame, seq)).expect("utf-8")
    }

    #[test]
    fn an_envelope_frame_has_id_data_and_a_terminating_blank_line() {
        let env = Envelope::event(
            "agent:tool_call",
            "dev-1-1",
            Some("task-1-1".into()),
            1234,
            serde_json::json!({ "name": "write_source" }),
        );
        let wire = text(&SseFrame::Envelope(env), 4);
        assert!(wire.starts_with("id: 1234-4\n"), "got {wire}");
        assert!(wire.contains("\ndata: {\"version\":1,"), "got {wire}");
        assert!(wire.ends_with("\n\n"), "frames must end with a blank line");
        assert_eq!(wire.lines().filter(|l| l.starts_with("data: ")).count(), 1);
        assert!(!wire.contains("event: "), "no SSE event: field by design");
    }

    #[test]
    fn a_comment_frame_is_a_comment_line() {
        let wire = text(&SseFrame::Comment("keep-alive".into()), 0);
        assert_eq!(wire, ": keep-alive\n\n");
    }

    #[test]
    fn the_hub_fans_out_to_every_subscriber() {
        let hub = SseHub::new(8);
        let mut a = hub.subscribe();
        let mut b = hub.subscribe();
        assert_eq!(hub.subscribers(), 2);
        assert_eq!(hub.publish(SseFrame::Comment("x".into())), 2);
        assert_eq!(a.try_recv(), Ok(SseFrame::Comment("x".into())));
        assert_eq!(b.try_recv(), Ok(SseFrame::Comment("x".into())));
    }

    #[test]
    fn the_sink_pushes_a_wrapped_event() {
        let hub = SseHub::new(8);
        let mut rx = hub.subscribe();
        let sink = HttpEventSink::new(Arc::clone(&hub), "dev-9-1");
        sink.emit("vm:state", serde_json::json!({ "running": true }));
        match rx.try_recv().expect("one frame") {
            SseFrame::Envelope(env) => {
                assert_eq!(env.kind, kind::EVENT);
                assert_eq!(env.event.as_deref(), Some("vm:state"));
                assert_eq!(env.agent_id, "dev-9-1");
                assert_eq!(env.payload["running"], true);
            }
            other => panic!("expected an envelope, got {other:?}"),
        }
    }
}
