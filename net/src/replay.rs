//! Replay protection: a per-peer high-water mark over a fixed window (v1.0 M4a).
//!
//! [connection.md §3.2](../../docs/connection.md) freezes this. The window is **five
//! minutes behind and one minute ahead**: the backward side is sized by the design's own
//! longest documented delay ([decisions §33](../../docs/decisions.md) allows a 30 s – 2 min
//! silent-retry period), and the forward side is clock skew and no more. The record is
//! **per peer and in memory**: the highest `ts` accepted from that peer, plus the set of
//! payload hashes accepted **at that same `ts`** — a message and its answer, or two
//! messages minted in the same millisecond, share a `ts` and are both legitimate.
//!
//! Two properties are worth stating where the code is, because both are easy to lose:
//!
//! - **Advancing the mark discards the set.** Cleanup is a discard, not a sweep: those
//!   hashes sit at a `ts` the window refuses anyway. Nothing accumulates and nothing
//!   needs a timer.
//! - **The record belongs to the peer, not to the key.** [decisions §13](../../docs/decisions.md)
//!   runs several keys in parallel during a rotation, so the record is keyed by `from`
//!   alone; a rotation must **not** reset it, or a replayed message would be accepted
//!   again the moment a new key appeared.
//!
//! **The honest limit**: the record is not persisted, so a restart forgets every mark
//! and a message still inside the window can be replayed **once** across a restart. The
//! exposure is bounded by the window, and it shrinks as traffic advances the record
//! again. Persisting it would make it a new on-disk format — a decision, not a detail.

use std::collections::HashMap;

/// How far behind "now" a `ts` may be: five minutes
/// ([connection.md §3.2](../../docs/connection.md)).
pub const REPLAY_WINDOW_BACK_MS: i64 = 5 * 60 * 1000;

/// How far ahead of "now" a `ts` may be: one minute, for clock skew.
pub const REPLAY_WINDOW_AHEAD_MS: i64 = 60 * 1000;

/// The window a [`ReplayGuard`] applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub back_ms: i64,
    pub ahead_ms: i64,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            back_ms: REPLAY_WINDOW_BACK_MS,
            ahead_ms: REPLAY_WINDOW_AHEAD_MS,
        }
    }
}

/// Why a message was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// Older than the window's backward side.
    Stale { ts: i64, now: i64, back_ms: i64 },
    /// Further into the future than the window allows.
    Future { ts: i64, now: i64, ahead_ms: i64 },
    /// Inside the window, but this peer has already had this `ts` — and, for the same
    /// `ts`, this payload.
    Replay { from: String, ts: i64 },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::Stale { ts, now, back_ms } => write!(
                f,
                "the timestamp is {} ms old; the window allows {back_ms}",
                now - ts
            ),
            ReplayError::Future { ts, now, ahead_ms } => write!(
                f,
                "the timestamp is {} ms in the future; the window allows {ahead_ms}",
                ts - now
            ),
            ReplayError::Replay { from, ts } => {
                write!(f, "{from} has already been seen at {ts}")
            }
        }
    }
}

impl std::error::Error for ReplayError {}

/// One peer's record: the highest `ts` accepted, and the payloads seen at it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PeerRecord {
    high_water: i64,
    at_water: std::collections::HashSet<String>,
}

/// The seen-message record, one per peer.
#[derive(Debug, Clone, Default)]
pub struct ReplayGuard {
    window: Window,
    peers: HashMap<String, PeerRecord>,
}

impl ReplayGuard {
    /// A guard with the frozen window.
    pub fn new() -> Self {
        Self {
            window: Window::default(),
            peers: HashMap::new(),
        }
    }

    /// A guard with a window of the caller's choosing — for tests and for a deployment
    /// that wants a different skew allowance. The *shape* is frozen; the numbers are not.
    pub fn with_window(window: Window) -> Self {
        Self {
            window,
            peers: HashMap::new(),
        }
    }

    /// The window in force.
    pub fn window(&self) -> Window {
        self.window
    }

    /// How many peers this guard is tracking.
    pub fn tracked_peers(&self) -> usize {
        self.peers.len()
    }

    /// The mark held for one peer, when it has one.
    pub fn high_water(&self, from: &str) -> Option<i64> {
        self.peers.get(from).map(|record| record.high_water)
    }

    /// Accept `(from, ts, body_hash)` at `now`, or say why not.
    ///
    /// The window is checked **before** the record is touched, so a stale or future
    /// message can never advance a peer's mark. On acceptance the mark advances to `ts`
    /// when `ts` is newer, and the set is replaced when it does — that replacement is
    /// the whole of the cleanup.
    pub fn accept(
        &mut self,
        from: &str,
        ts: i64,
        body_hash: &str,
        now: i64,
    ) -> Result<(), ReplayError> {
        if ts < now - self.window.back_ms {
            return Err(ReplayError::Stale {
                ts,
                now,
                back_ms: self.window.back_ms,
            });
        }
        if ts > now + self.window.ahead_ms {
            return Err(ReplayError::Future {
                ts,
                now,
                ahead_ms: self.window.ahead_ms,
            });
        }
        let record = self.peers.entry(from.to_string()).or_default();
        if ts < record.high_water {
            return Err(ReplayError::Replay {
                from: from.to_string(),
                ts,
            });
        }
        if ts == record.high_water && record.at_water.contains(body_hash) {
            return Err(ReplayError::Replay {
                from: from.to_string(),
                ts,
            });
        }
        if ts > record.high_water {
            record.high_water = ts;
            record.at_water.clear();
        }
        record.at_water.insert(body_hash.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    #[test]
    fn a_fresh_message_is_accepted_and_a_second_one_at_the_same_millisecond_is_too() {
        let mut guard = ReplayGuard::new();
        assert!(guard.accept("dev-a", NOW, "hash-1", NOW).is_ok());
        assert!(guard.accept("dev-a", NOW, "hash-2", NOW).is_ok());
        assert_eq!(guard.high_water("dev-a"), Some(NOW));
    }

    #[test]
    fn the_window_refuses_stale_and_future_timestamps() {
        let mut guard = ReplayGuard::new();
        assert!(matches!(
            guard.accept("dev-a", NOW - REPLAY_WINDOW_BACK_MS - 1, "h", NOW),
            Err(ReplayError::Stale { .. })
        ));
        assert!(matches!(
            guard.accept("dev-a", NOW + REPLAY_WINDOW_AHEAD_MS + 1, "h", NOW),
            Err(ReplayError::Future { .. })
        ));
        // The boundaries themselves are inside.
        assert!(guard
            .accept("dev-a", NOW - REPLAY_WINDOW_BACK_MS, "h", NOW)
            .is_ok());
        assert!(guard
            .accept("dev-b", NOW + REPLAY_WINDOW_AHEAD_MS, "h", NOW)
            .is_ok());
    }

    #[test]
    fn a_rejected_timestamp_never_advances_the_mark() {
        let mut guard = ReplayGuard::new();
        guard.accept("dev-a", NOW, "h", NOW).expect("accept");
        assert!(guard
            .accept("dev-a", NOW - 10 * 60 * 1000, "h2", NOW)
            .is_err());
        assert_eq!(guard.high_water("dev-a"), Some(NOW), "the mark held");
    }
}
