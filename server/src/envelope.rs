//! The control-plane's own frames, built on the one envelope.
//!
//! The envelope's shape lives in `host-core/src/events.rs` — the kernel emits, every
//! transport wraps, and there is exactly one place that writes the fields down.
//! What this module adds is the vocabulary the control plane puts *in* it: the
//! opening `hello`, and the reserved `gap`.

use host_core::events::{envelope, kind, Envelope};

/// The envelope type every frame carries (re-exported: the control plane does not
/// define its own).
pub use host_core::events::{Envelope as Frame, ENVELOPE_VERSION};

/// The `filters` object a `hello` frame carries: **the one place its vocabulary is written down**
/// (v1.0 M6-3b).
///
/// `task_id` is echoed back from what the connection asked for, because it is the one parameter the
/// server **enforces** (`docs/control-plane-events.md` §4). `event` and `agent_id` are still
/// advertised as the empty defaults they have always been: they are accepted and ignored until
/// their own batch, so a client that sends them sees no change.
pub fn filters(task_id: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "event": [],
        "agent_id": null,
        "task_id": task_id,
    })
}

/// The frame a client receives first on `/v0/events`.
///
/// `buffer` describes what the stream can replay, and `filters` echoes what this connection asked
/// for.
pub fn hello(agent_id: &str) -> Envelope {
    envelope(
        kind::HELLO,
        None,
        agent_id,
        None,
        serde_json::json!({
            "buffer": { "from": 0, "to": 0 },
            "filters": filters(None),
        }),
    )
}

/// The reserved `gap` frame kind. **Not implemented yet**: a subscriber that
/// falls behind loses the frames it missed and is not told (`docs/control-plane-events.md` §2).
pub const GAP_KIND: &str = kind::GAP;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_is_the_documented_shape() {
        let env = hello("local-4711-1");
        assert_eq!(env.version, ENVELOPE_VERSION);
        assert_eq!(env.kind, kind::HELLO);
        assert!(env.event.is_none());
        assert!(env.task_id.is_none());
        assert_eq!(env.agent_id, "local-4711-1");
        assert_eq!(env.payload["buffer"]["from"], 0);
        assert!(env.payload["filters"]["event"].is_array());
        assert!(env.ts > 0);
    }

    #[test]
    fn the_gap_kind_is_reserved_but_unused() {
        assert_eq!(GAP_KIND, "gap");
    }
}
