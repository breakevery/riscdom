//! The SSE push side: the hub that fans events out, the frame format, the bounded
//! replay buffer, and the `host::EventSink` implementation that feeds it.
//!
//! The wire format is settled in `docs/control-plane-events.md` §1: a frame uses
//! `id:` and `data:` only, and ends with a blank line. There is deliberately no
//! `event:` field — that would break a browser's single `onmessage` handler, and
//! the envelope's `event` field is the router instead. The envelope itself comes
//! from `host::events`, so every transport writes the same one.
//!
//! **Replay.** Every frame carries `id: <ts>-<seq>` where `seq` is a
//! **server-wide** ordinal (not a per-connection one): that is what makes the id
//! usable as a reconnect cursor. The hub keeps the last
//! [`REPLAY_CAPACITY`] frames in memory, and a client that reconnects with
//! `Last-Event-ID` gets everything after it — or a `gap` frame when the hole is
//! older than the buffer.

use host::events::{envelope, event_envelope, Envelope};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

/// Default number of frames a slow **subscriber** may fall behind before it is
/// told it lagged. Bounded on purpose: an unbounded buffer is a memory leak with
/// a friendly name.
pub const CHANNEL_CAPACITY: usize = 1024;

/// How many recent frames the hub keeps for `Last-Event-ID` replay.
///
/// In memory only, and bounded: replay is best-effort, and a client that asks for
/// something older than this is told so with a `gap` frame instead of being
/// served a silent hole.
pub const REPLAY_CAPACITY: usize = 1024;

/// One frame, ready to write.
#[derive(Debug, Clone)]
pub struct WireFrame {
    /// The replay cursor (`None` for a comment, which is never replayed).
    pub id: Option<String>,
    /// The frame exactly as it goes on the wire, blank line included.
    pub bytes: Vec<u8>,
    /// The ordinal behind `id`.
    pub seq: u64,
}

impl WireFrame {
    /// The `id:` value, when there is one.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

/// What a reconnecting client is owed.
#[derive(Debug, Clone)]
pub enum Replay {
    /// Nothing to replay: a fresh stream, or the caller is already current.
    Nothing,
    /// These frames, then live.
    Frames(Vec<Arc<WireFrame>>),
    /// The hole is older than the buffer: `lost_after` is the oldest id still
    /// held, and the frames from there on follow.
    Gap {
        lost_after: String,
        frames: Vec<Arc<WireFrame>>,
    },
}

/// Serialise an envelope frame. `id` is the cursor the client stores.
fn envelope_bytes(envelope: &Envelope, id: &str) -> Vec<u8> {
    format!("id: {id}\ndata: {}\n\n", envelope.to_json()).into_bytes()
}

/// Serialise a heartbeat: an SSE comment, which every client ignores.
fn comment_bytes(text: &str) -> Vec<u8> {
    format!(": {text}\n\n").into_bytes()
}

/// The fan-out point: every SSE subscriber attaches here.
pub struct SseHub {
    tx: broadcast::Sender<Arc<WireFrame>>,
    buffer: Mutex<VecDeque<Arc<WireFrame>>>,
    seq: AtomicU64,
}

impl SseHub {
    /// Create the hub. The returned handle is shared: it is the sink side and the
    /// subscribe side at once.
    pub fn new(capacity: usize) -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(capacity);
        Arc::new(Self {
            tx,
            buffer: Mutex::new(VecDeque::with_capacity(REPLAY_CAPACITY)),
            seq: AtomicU64::new(0),
        })
    }

    /// Attach a subscriber.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<WireFrame>> {
        self.tx.subscribe()
    }

    /// How many subscribers are attached right now (the `/v0/status` figure).
    pub fn subscribers(&self) -> usize {
        self.tx.receiver_count()
    }

    /// The next ordinal. Server-wide, so ids stay comparable across connections.
    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Wrap an event, remember it for replay, and push it to every subscriber.
    pub fn publish_event(&self, envelope: Envelope) -> Arc<WireFrame> {
        let seq = self.next_seq();
        let id = format!("{}-{seq}", envelope.ts);
        let frame = Arc::new(WireFrame {
            id: Some(id.clone()),
            bytes: envelope_bytes(&envelope, &id),
            seq,
        });
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.push_back(Arc::clone(&frame));
            while buffer.len() > REPLAY_CAPACITY {
                buffer.pop_front();
            }
        }
        let _ = self.tx.send(Arc::clone(&frame));
        frame
    }

    /// Push a heartbeat. Comments are never buffered: they carry nothing.
    pub fn publish_comment(&self, text: &str) {
        let frame = Arc::new(WireFrame {
            id: None,
            bytes: comment_bytes(text),
            seq: self.next_seq(),
        });
        let _ = self.tx.send(frame);
    }

    /// The frame a client receives first: `hello`, describing what the buffer
    /// holds. Per connection, and never buffered itself.
    pub fn hello(&self, agent_id: &str) -> Arc<WireFrame> {
        let (from, to) = match self.buffer.lock() {
            Ok(buffer) => (
                buffer.front().map(|f| f.seq).unwrap_or(0),
                buffer.back().map(|f| f.seq).unwrap_or(0),
            ),
            Err(_) => (0, 0),
        };
        let envelope = envelope(
            host::events::kind::HELLO,
            None,
            agent_id,
            None,
            serde_json::json!({
                "buffer": { "from": from, "to": to },
                "filters": { "event": [], "agent_id": null, "task_id": null },
            }),
        );
        let seq = self.next_seq();
        let id = format!("{}-{seq}", envelope.ts);
        Arc::new(WireFrame {
            id: Some(id.clone()),
            bytes: envelope_bytes(&envelope, &id),
            seq,
        })
    }

    /// The frame that says "the hole could not be filled".
    pub fn gap(&self, lost_after: &str) -> Arc<WireFrame> {
        let envelope = envelope(
            host::events::kind::GAP,
            None,
            "server",
            None,
            serde_json::json!({ "lost_after": lost_after }),
        );
        let seq = self.next_seq();
        let id = format!("{}-{seq}", envelope.ts);
        Arc::new(WireFrame {
            id: Some(id.clone()),
            bytes: envelope_bytes(&envelope, &id),
            seq,
        })
    }

    /// What a client that reconnects with `Last-Event-ID` is owed.
    pub fn replay_after(&self, last_event_id: Option<&str>) -> Replay {
        let Some(requested) = last_event_id else {
            return Replay::Nothing;
        };
        let Ok(requested_seq) = parse_seq(requested) else {
            // An id this server did not mint (or a client that invented one):
            // nothing can be proven about the hole, so say so and replay all.
            return self.replay_all();
        };
        let buffer = match self.buffer.lock() {
            Ok(buffer) => buffer,
            Err(_) => return Replay::Nothing,
        };
        let (Some(oldest), Some(newest)) = (buffer.front(), buffer.back()) else {
            // Nothing has happened yet: there is no hole to fill.
            return Replay::Nothing;
        };
        if requested_seq >= newest.seq {
            return Replay::Nothing;
        }
        if requested_seq + 1 < oldest.seq {
            return Replay::Gap {
                lost_after: oldest.id.clone().unwrap_or_default(),
                frames: buffer.iter().cloned().collect(),
            };
        }
        Replay::Frames(
            buffer
                .iter()
                .filter(|f| f.seq > requested_seq)
                .cloned()
                .collect(),
        )
    }

    /// Replay the whole buffer, as a gap (used when the id cannot be ordered).
    fn replay_all(&self) -> Replay {
        match self.buffer.lock() {
            Ok(buffer) if !buffer.is_empty() => Replay::Gap {
                lost_after: buffer
                    .front()
                    .and_then(|f| f.id.clone())
                    .unwrap_or_default(),
                frames: buffer.iter().cloned().collect(),
            },
            _ => Replay::Nothing,
        }
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
            hub.publish_comment(text);
        });
    }
}

/// The ordinal out of an id this server minted: `<ts>-<seq>`.
fn parse_seq(id: &str) -> Result<u64, ()> {
    id.rsplit_once('-')
        .and_then(|(_, seq)| seq.parse().ok())
        .ok_or(())
}

/// Brings the host's events to the stream, wrapped in the one envelope.
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
        self.hub
            .publish_event(event_envelope(event, &self.agent_id, payload));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use host::events::kind;
    use host::EventSink;

    fn event(name: &str) -> Envelope {
        event_envelope(name, "dev-1-1", serde_json::json!({ "name": name }))
    }

    #[test]
    fn an_envelope_frame_has_id_data_and_a_terminating_blank_line() {
        let hub = SseHub::new(8);
        let frame = hub.publish_event(event("agent:tool_call"));
        let wire = String::from_utf8(frame.bytes.clone()).expect("utf-8");
        assert!(wire.starts_with("id: "), "got {wire}");
        assert!(wire.contains("\ndata: {\"version\":1,"), "got {wire}");
        assert!(wire.ends_with("\n\n"), "frames must end with a blank line");
        assert_eq!(wire.lines().filter(|l| l.starts_with("data: ")).count(), 1);
        assert!(!wire.contains("event: "), "no SSE event: field by design");
        // The id is `<ts>-<seq>` and the seq is the frame's own ordinal.
        let id = frame.id().expect("an id");
        assert_eq!(id.rsplit_once('-').expect("split").1, frame.seq.to_string());
    }

    #[test]
    fn a_comment_frame_is_a_comment_line_and_is_not_buffered() {
        let hub = SseHub::new(8);
        let mut rx = hub.subscribe();
        hub.publish_comment("keep-alive");
        let frame = rx.try_recv().expect("one frame");
        assert_eq!(
            String::from_utf8(frame.bytes.clone()).expect("utf-8"),
            ": keep-alive\n\n"
        );
        assert!(frame.id().is_none());
        assert!(matches!(hub.replay_after(Some("x-1")), Replay::Nothing));
    }

    #[test]
    fn the_hub_fans_out_to_every_subscriber() {
        let hub = SseHub::new(8);
        let mut a = hub.subscribe();
        let mut b = hub.subscribe();
        assert_eq!(hub.subscribers(), 2);
        hub.publish_event(event("vm:state"));
        assert!(a.try_recv().is_ok());
        assert!(b.try_recv().is_ok());
    }

    #[test]
    fn the_sink_pushes_a_wrapped_event() {
        let hub = SseHub::new(8);
        let mut rx = hub.subscribe();
        let sink = HttpEventSink::new(Arc::clone(&hub), "dev-9-1");
        sink.emit("vm:state", serde_json::json!({ "running": true }));
        let frame = rx.try_recv().expect("one frame");
        let wire = String::from_utf8(frame.bytes.clone()).expect("utf-8");
        assert!(wire.contains("\"kind\":\"event\""), "{wire}");
        assert!(wire.contains("\"event\":\"vm:state\""), "{wire}");
        assert!(wire.contains("\"agent_id\":\"dev-9-1\""), "{wire}");
        assert!(wire.contains("\"version\":1"), "{wire}");
        assert_eq!(kind::EVENT, "event");
    }

    #[test]
    fn replay_returns_only_what_follows_the_cursor() {
        let hub = SseHub::new(8);
        let first = hub.publish_event(event("a"));
        let second = hub.publish_event(event("b"));
        let third = hub.publish_event(event("c"));

        // Already current: nothing to send.
        assert!(matches!(
            hub.replay_after(Some(third.id().expect("id"))),
            Replay::Nothing
        ));
        // One behind: exactly the last frame.
        match hub.replay_after(Some(second.id().expect("id"))) {
            Replay::Frames(frames) => {
                assert_eq!(frames.len(), 1);
                assert_eq!(frames[0].id(), third.id());
            }
            other => panic!("got {other:?}"),
        }
        // Two behind: both of the later frames, in order.
        match hub.replay_after(Some(first.id().expect("id"))) {
            Replay::Frames(frames) => {
                assert_eq!(frames.len(), 2);
                assert_eq!(frames[0].id(), second.id());
                assert_eq!(frames[1].id(), third.id());
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_cursor_older_than_the_buffer_is_a_gap() {
        let hub = SseHub::new(8);
        let first = hub.publish_event(event("oldest"));
        for i in 0..(REPLAY_CAPACITY + 5) {
            hub.publish_event(event(&format!("filler-{i}")));
        }
        // `first` has fallen out of the buffer, so the hole cannot be filled.
        match hub.replay_after(Some(first.id().expect("id"))) {
            Replay::Gap { lost_after, frames } => {
                assert!(!lost_after.is_empty());
                assert_eq!(frames.len(), REPLAY_CAPACITY, "the buffer is bounded");
                assert_eq!(
                    frames[0].id(),
                    Some(lost_after.as_str()),
                    "replay resumes at the oldest id held"
                );
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn an_id_this_server_never_minted_is_a_gap() {
        let hub = SseHub::new(8);
        hub.publish_event(event("a"));
        match hub.replay_after(Some("not-ours")) {
            Replay::Gap { frames, .. } => assert_eq!(frames.len(), 1),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_fresh_stream_replays_nothing() {
        let hub = SseHub::new(8);
        hub.publish_event(event("a"));
        assert!(matches!(hub.replay_after(None), Replay::Nothing));
    }

    #[test]
    fn hello_describes_the_buffer() {
        let hub = SseHub::new(8);
        hub.publish_event(event("a"));
        let last = hub.publish_event(event("b"));
        let hello = hub.hello("dev-1-1");
        let wire = String::from_utf8(hello.bytes.clone()).expect("utf-8");
        assert!(wire.contains("\"kind\":\"hello\""), "{wire}");
        assert!(wire.contains("\"from\":1"), "{wire}");
        assert!(wire.contains(&format!("\"to\":{}", last.seq)), "{wire}");
    }
}
