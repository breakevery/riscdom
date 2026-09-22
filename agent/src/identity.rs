//! Agent identity (v0.8 batch B).
//!
//! Every audit event names the agent that caused it. The shape is
//! `<device>-<pid>-<seq>`:
//!
//! - **device** — which machine. The single-machine stage has one: [`DEVICE`];
//! - **pid** — which process, so two processes writing one chain cannot collide;
//! - **seq** — which agent *inside* that process, handed out by [`next_agent_id`]
//!   from a process-wide counter, so two `AppState`s (or two `AgentLoop`s) in one
//!   process never share an identity.
//!
//! Example: `local-12345-1`. The value is an audit field, not a secret: it names
//! an agent, and it is what lets one chain say "who made whom do what" once
//! several agents write to it.

use std::sync::atomic::{AtomicU64, Ordering};

/// The device part of an identity. Single-machine stage: one constant.
pub const DEVICE: &str = "local";

/// Which agent in this process; the first one is `seq` 1.
static SEQ: AtomicU64 = AtomicU64::new(0);

/// Mint the next agent identity in this process: `<device>-<pid>-<seq>`.
///
/// Monotonic and process-wide: every call returns a value no other call (here or
/// in another process) returns.
pub fn next_agent_id() -> String {
    let seq = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    format!("{DEVICE}-{}-{seq}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_shaped_as_documented() {
        let a = next_agent_id();
        let b = next_agent_id();
        assert_ne!(a, b, "two ids in one process must differ");
        for id in [&a, &b] {
            let parts: Vec<&str> = id.split('-').collect();
            assert_eq!(parts.len(), 3, "device-pid-seq: {id}");
            assert_eq!(parts[0], DEVICE);
            assert_eq!(parts[1], std::process::id().to_string());
            assert!(
                parts[2].parse::<u64>().is_ok(),
                "the sequence is a number: {id}"
            );
        }
    }
}
