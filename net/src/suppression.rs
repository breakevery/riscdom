//! Suppression: the three layers that stop a takeover firing on one node's suspicion (v1.0 M5-3a).
//!
//! [decisions §33](../../docs/decisions.md) freezes the shape and [decisions §127](../../docs/decisions.md)
//! point 4 authorises it: **all three layers are required, and none is optional** —
//!
//! 1. a **waiting period** (30 s – 2 min of silent retries);
//! 2. **global confirmation** (most nodes must report "cannot reach the centre" before anything fires);
//! 3. **backoff plus precedence** (the first in line waits a random backoff and stands down the moment it
//!    sees a takeover broadcast).
//!
//! This module is that mechanism and its numbers — **the layers, not the takeover**. What a node does once
//! it is through them (opening a temporary segment, taking over) is M5-3b, and `StandingIn` is the one phase
//! below that M5-3a never enters.
//!
//! **The confirmation is §6.7's rule, not a new one.** `net::WitnessTable` already answers "does every
//! witness that remains say this node is gone, with a witness of life vetoing?", and [connection.md
//! §6.7](../../docs/connection.md) argues at length why a **majority would be wrong exactly where it
//! matters** (in a partition it would declare a live node gone). The centre is a `node_id` like any other, so
//! the same table decides about it: [`Suppression::confirm`] calls the same [`WitnessTable::judge`].
//!
//! **Dependency direction.** Nothing here writes an audit row or names a capability: the machine is local
//! state, and the reports its confirmation reads travel by §6.7's mechanism. Whether a node is *first in
//! line* is §33's precedence — pinned by the owner to **`node_id` order** for v1.0 (no registration-frame
//! field), so [`first_in_line`] is a sort and not a protocol.

use crate::liveness::{Judgement, Report, WitnessTable};
use crate::message::PROTOCOL_VERSION;
use serde_json::Value;
use std::time::Duration;

/// The first layer's window: how long a node silently retries before it will even ask the group
/// (v1.0 M5-3a). §33's range is 30 s – 2 min; 60 s is the default, and it is **longer than §6.6's 45 s
/// online window on purpose** — a node that started waiting while its own centre would still call it online
/// would be waiting on a fact the other side has not reached yet.
pub const SUPPRESSION_WAIT: Duration = Duration::from_secs(60);

/// The third layer's window: the longest a first-in-line node backs off before it would take over
/// (v1.0 M5-3a). **Shorter than [`SUPPRESSION_WAIT`]** so a node that lost the race hears the winner inside
/// its own wait, and inside §33's 30 s – 2 min envelope.
pub const SUPPRESSION_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Where a node is in §33's suppression (v1.0 M5-3a).
///
/// The first four are this batch's; **`StandingIn` is M5-3b's** — M5-3a stops at `BackingOff`, because
/// taking over is the next batch's act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionPhase {
    /// The centre is reachable, or came back: nothing to suppress.
    Candidate,
    /// Layer one: this node cannot reach the centre, and is silently retrying.
    Waiting,
    /// Layer two: the wait elapsed, and the group has not yet confirmed.
    Confirming,
    /// Layer three: the group confirmed; this node is inside its backoff window.
    BackingOff,
    /// **M5-3b.** Past the backoff and standing in for the centre.
    StandingIn,
}

impl SuppressionPhase {
    /// The word for it.
    pub fn as_str(self) -> &'static str {
        match self {
            SuppressionPhase::Candidate => "candidate",
            SuppressionPhase::Waiting => "waiting",
            SuppressionPhase::Confirming => "confirming",
            SuppressionPhase::BackingOff => "backing-off",
            SuppressionPhase::StandingIn => "standing-in",
        }
    }
}

/// One node's suppression state (v1.0 M5-3a).
///
/// Local, in memory, and written to no chain — like §6.7's [`crate::Prober`] view, a suspicion is not an
/// event. The machine is driven by two inputs and one supplier: **the node's own observation** of the centre
/// ([`Self::observe`]), **the group's confirmation** ([`Self::confirm`], §6.7's table), and **a takeover
/// broadcast** ([`Self::stand_down`]) — the last of which is M5-3b's to deliver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suppression {
    node_id: String,
    phase: SuppressionPhase,
    since_ms: i64,
    /// When a backoff ends, while the phase is `BackingOff`.
    deadline_ms: Option<i64>,
}

impl Suppression {
    /// A node that has not yet noticed anything.
    pub fn new(node_id: &str, now: i64) -> Self {
        Self {
            node_id: node_id.to_string(),
            phase: SuppressionPhase::Candidate,
            since_ms: now,
            deadline_ms: None,
        }
    }

    /// The node this machine belongs to.
    pub fn node_id(&self) -> &str {
        self.node_id.as_str()
    }

    /// Where it is now.
    pub fn phase(&self) -> SuppressionPhase {
        self.phase
    }

    /// When the current phase began.
    pub fn since_ms(&self) -> i64 {
        self.since_ms
    }

    /// When the backoff ends, while the node is backing off.
    pub fn deadline_ms(&self) -> Option<i64> {
        self.deadline_ms
    }

    /// **Layer one.** What the node itself sees right now: can it reach the centre?
    ///
    /// A centre that is reachable puts the machine back at [`SuppressionPhase::Candidate`] from *any* phase —
    /// §33's "ending the period means returning to the centre", and the same reason §6.7 clears its
    /// judgements the moment a witness of life speaks. A centre that is not reachable starts the wait; once
    /// [`SUPPRESSION_WAIT`] has elapsed it moves to [`SuppressionPhase::Confirming`] and stops there — the
    /// second layer is the group's, and it is [`Self::confirm`]'s to answer.
    pub fn observe(&mut self, centre_reachable: bool, now: i64) -> SuppressionPhase {
        if centre_reachable {
            self.phase = SuppressionPhase::Candidate;
            self.since_ms = now;
            self.deadline_ms = None;
            return self.phase;
        }
        match self.phase {
            SuppressionPhase::Candidate => {
                self.phase = SuppressionPhase::Waiting;
                self.since_ms = now;
            }
            SuppressionPhase::Waiting
                if now.saturating_sub(self.since_ms) >= SUPPRESSION_WAIT.as_millis() as i64 =>
            {
                self.phase = SuppressionPhase::Confirming;
                self.since_ms = now;
            }
            _ => {}
        }
        self.phase
    }

    /// **Layer two.** §6.7's rule, applied to the centre — the same table, the same unanimity, the same
    /// vetoing witness of life.
    ///
    /// Answers the judgement when one is reached, and moves to [`SuppressionPhase::BackingOff`] with a
    /// backoff deadline. Answers `None` while the group has not confirmed (a veto, a stale report, or too few
    /// witnesses all read the same way here: **not yet**). Does nothing outside
    /// [`SuppressionPhase::Confirming`] — a node may not skip its own wait.
    pub fn confirm(
        &mut self,
        table: &WitnessTable,
        centre: &str,
        now: i64,
        witness_online: impl Fn(&str) -> bool,
    ) -> Option<Judgement> {
        if self.phase != SuppressionPhase::Confirming {
            return None;
        }
        let judgement = table.judge(centre, now, witness_online)?;
        self.phase = SuppressionPhase::BackingOff;
        self.since_ms = now;
        self.deadline_ms = Some(now.saturating_add(backoff_delay_ms(&self.node_id, now)));
        Some(judgement)
    }

    /// **Layer three, the stand-down half.** A takeover broadcast arrived: whatever this node was doing, it
    /// is a candidate again (§33: "stands down the moment it sees a takeover broadcast").
    pub fn stand_down(&mut self, now: i64) {
        self.phase = SuppressionPhase::Candidate;
        self.since_ms = now;
        self.deadline_ms = None;
    }

    /// Whether the backoff has elapsed — the point past which M5-3b's takeover would begin.
    pub fn backoff_elapsed(&self, now: i64) -> bool {
        matches!((self.phase, self.deadline_ms), (SuppressionPhase::BackingOff, Some(deadline)) if now >= deadline)
    }
}

/// **Layer three, the precedence half**: who is first in line (§33's precedence, pinned by the owner to
/// `node_id` order for v1.0 — no registration-frame field).
///
/// The smallest `node_id` among the candidates. `None` when there are none to order.
pub fn first_in_line<'a>(nodes: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    nodes.into_iter().min()
}

/// Is `node_id` the first in line, given the others it could be in line with?
///
/// It is first when nothing sorts before it — including when it is alone, which is the honest answer: §33
/// suppresses a *group's* takeover, and a node that knows no peers is first in the only line it knows.
pub fn is_first_in_line<'a>(node_id: &str, others: impl IntoIterator<Item = &'a str>) -> bool {
    !others.into_iter().any(|other| other < node_id)
}

/// How long a first-in-line node waits before it would take over (v1.0 M5-3a).
///
/// Inside `[0, SUPPRESSION_BACKOFF_MAX]`, and **deterministic in `(node_id, now)`** — a hash rather than a
/// random number generator, so the window is testable and no dependency is added. Two nodes that confirm in
/// the same millisecond draw different delays (their `node_id`s differ), and the whole point is that the
/// loser hears the winner inside its own wait.
pub fn backoff_delay_ms(node_id: &str, now: i64) -> i64 {
    // FNV-1a over the node's name and the moment, then folded into the window.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in node_id.as_bytes().iter().chain(now.to_le_bytes().iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let window = SUPPRESSION_BACKOFF_MAX.as_millis() as u64;
    (hash % (window + 1)) as i64
}

/// A **centre report** out of a body, when the body is a §6.7 report **about `centre`** (v1.0 M5-3b-1).
///
/// Nodes send each other the same `{"unreachable": …}` / `{"reachable": …}` bodies §6.7 sends upward, so
/// the only new thing on the receiving side is the filter: a report about some other peer is that peer's
/// business, and a report about the centre is the suppression's.
pub fn centre_report_from(body: &Value, centre: &str) -> Option<Report> {
    crate::liveness::report_of(body).filter(|report| report.node_id() == centre)
}

/// A **takeover broadcast**: a node announcing that it is standing in for the centre (v1.0 M5-3b-1).
///
/// §33's third layer is "the first in line waits a random backoff and **stands down the moment it sees a
/// takeover broadcast**", so the broadcast is what the third layer reacts to. It travels **between peers**
/// (the same workgroup, point to point) and never to the cross-region server: the nodes that must stand down
/// are the ones in the workgroup, and the server is not in one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Takeover {
    /// The centre being stood in for.
    pub centre: String,
    /// The node doing it.
    pub by: String,
    /// When it said so (the sender's clock).
    pub at_ms: i64,
}

/// The body a takeover broadcast travels as (v1.0 M5-3b-1).
///
/// An ordinary §3 frame's body, like §6.6's registration and §6.7's reports — the shape is this batch's.
pub fn takeover_body(centre: &str, by: &str, at_ms: i64) -> Value {
    serde_json::json!({
        "takeover": PROTOCOL_VERSION,
        "centre": centre,
        "by": by,
        "at_ms": at_ms,
    })
}

/// Read a takeover broadcast out of a body, when the body is one.
pub fn is_takeover(body: &Value) -> Option<Takeover> {
    if body.get("takeover").and_then(Value::as_u64) != Some(u64::from(PROTOCOL_VERSION)) {
        return None;
    }
    Some(Takeover {
        centre: body.get("centre").and_then(Value::as_str)?.to_string(),
        by: body.get("by").and_then(Value::as_str)?.to_string(),
        at_ms: body.get("at_ms").and_then(Value::as_i64)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::liveness::REPORT_WINDOW_MS;

    const T0: i64 = 1_700_000_000_000;
    const WAIT_MS: i64 = 60_000;

    #[test]
    fn layer_one_waits_the_full_period_before_confirming() {
        let mut machine = Suppression::new("dev-a", T0);
        assert_eq!(machine.phase(), SuppressionPhase::Candidate);
        // One node's own silence is not enough: the wait starts...
        assert_eq!(machine.observe(false, T0), SuppressionPhase::Waiting);
        // ...and is not over a second short.
        assert_eq!(
            machine.observe(false, T0 + WAIT_MS - 1),
            SuppressionPhase::Waiting
        );
        assert_eq!(
            machine.observe(false, T0 + WAIT_MS),
            SuppressionPhase::Confirming
        );
        // A centre that answers puts it straight back — from any phase.
        assert_eq!(
            machine.observe(true, T0 + WAIT_MS + 1),
            SuppressionPhase::Candidate
        );
    }

    #[test]
    fn layer_two_is_the_witness_rule_and_needs_unanimity() {
        let mut machine = Suppression::new("dev-a", T0);
        machine.observe(false, T0);
        machine.observe(false, T0 + WAIT_MS);
        assert_eq!(machine.phase(), SuppressionPhase::Confirming);

        // A witness of life vetoes: still confirming.
        let table = WitnessTable::new();
        table.record("dev-b", "centre", false, T0 + WAIT_MS);
        table.record("dev-c", "centre", true, T0 + WAIT_MS);
        assert_eq!(
            machine.confirm(&table, "centre", T0 + WAIT_MS, |_| true),
            None
        );
        assert_eq!(machine.phase(), SuppressionPhase::Confirming);

        // Unanimous among the witnesses that remain: judged, and the machine backs off.
        let unanimous = WitnessTable::new();
        unanimous.record("dev-b", "centre", false, T0 + WAIT_MS);
        unanimous.record("dev-c", "centre", false, T0 + WAIT_MS);
        let judgement = machine
            .confirm(&unanimous, "centre", T0 + WAIT_MS, |_| true)
            .expect("judged");
        assert_eq!(judgement.peer, "centre");
        assert_eq!(machine.phase(), SuppressionPhase::BackingOff);
        assert!(machine.deadline_ms().is_some());

        // And a stale report is not a witness.
        let stale = WitnessTable::new();
        stale.record("dev-b", "centre", false, T0);
        let mut fresh = Suppression::new("dev-a", T0);
        fresh.observe(false, T0);
        fresh.observe(false, T0 + WAIT_MS);
        assert_eq!(
            fresh.confirm(
                &stale,
                "centre",
                T0 + WAIT_MS + REPORT_WINDOW_MS + 1,
                |_| true
            ),
            None
        );
    }

    #[test]
    fn layer_two_cannot_be_skipped() {
        let mut machine = Suppression::new("dev-a", T0);
        let table = WitnessTable::new();
        table.record("dev-b", "centre", false, T0);
        // Still a candidate: the group's word does not replace the node's own wait.
        assert_eq!(machine.confirm(&table, "centre", T0, |_| true), None);
        assert_eq!(machine.phase(), SuppressionPhase::Candidate);
    }

    #[test]
    fn layer_three_backs_off_inside_the_window_and_stands_down_on_a_broadcast() {
        let mut machine = Suppression::new("dev-a", T0);
        machine.observe(false, T0);
        machine.observe(false, T0 + WAIT_MS);
        let table = WitnessTable::new();
        table.record("dev-b", "centre", false, T0 + WAIT_MS);
        machine
            .confirm(&table, "centre", T0 + WAIT_MS, |_| true)
            .expect("judged");
        let deadline = machine.deadline_ms().expect("a deadline");
        assert!((T0 + WAIT_MS..=T0 + WAIT_MS + 30_000).contains(&deadline));
        assert!(!machine.backoff_elapsed(T0 + WAIT_MS));
        assert!(machine.backoff_elapsed(deadline));

        // The broadcast arrives: the machine is a candidate again, at once.
        machine.stand_down(deadline);
        assert_eq!(machine.phase(), SuppressionPhase::Candidate);
        assert_eq!(machine.deadline_ms(), None);
    }

    #[test]
    fn the_backoff_window_is_bounded_and_deterministic() {
        for node in ["dev-a", "dev-b", "node-9"] {
            for now in [T0, T0 + 1, T0 + 999] {
                let delay = backoff_delay_ms(node, now);
                assert!((0..=30_000).contains(&delay), "{delay}");
                assert_eq!(delay, backoff_delay_ms(node, now), "deterministic");
            }
        }
        // Different nodes do not draw the same delay in the same millisecond.
        let a = backoff_delay_ms("dev-a", T0);
        let b = backoff_delay_ms("dev-b", T0);
        assert_ne!(a, b, "two nodes, two draws");
    }

    #[test]
    fn precedence_is_node_id_order() {
        assert_eq!(first_in_line(["dev-c", "dev-a", "dev-b"]), Some("dev-a"));
        assert_eq!(first_in_line(Vec::<&str>::new()), None);
        assert!(is_first_in_line("dev-a", ["dev-b", "dev-c"]));
        assert!(!is_first_in_line("dev-b", ["dev-a"]));
        // Alone: first in the only line it knows.
        assert!(is_first_in_line("dev-a", Vec::<&str>::new()));
    }

    #[test]
    fn the_machine_never_stands_in_by_itself() {
        // M5-3a stops at the backoff: `StandingIn` is M5-3b's act, and nothing here reaches it.
        let mut machine = Suppression::new("dev-a", T0);
        machine.observe(false, T0);
        machine.observe(false, T0 + WAIT_MS);
        let table = WitnessTable::new();
        table.record("dev-b", "centre", false, T0 + WAIT_MS);
        machine
            .confirm(&table, "centre", T0 + WAIT_MS, |_| true)
            .expect("judged");
        assert_eq!(machine.phase(), SuppressionPhase::BackingOff);
        assert_ne!(machine.phase(), SuppressionPhase::StandingIn);
        assert_eq!(SuppressionPhase::StandingIn.as_str(), "standing-in");
    }

    #[test]
    fn the_takeover_body_round_trips_and_is_not_confused_with_anything_else() {
        let body = takeover_body("centre", "dev-a", T0);
        assert_eq!(
            is_takeover(&body),
            Some(Takeover {
                centre: "centre".to_string(),
                by: "dev-a".to_string(),
                at_ms: T0
            })
        );
        // §6.7's bodies are not broadcasts, and one member missing means the body is not one either.
        assert_eq!(is_takeover(&crate::liveness::probe_body()), None);
        assert_eq!(
            is_takeover(&crate::liveness::unreachable_body("centre")),
            None
        );
        assert_eq!(is_takeover(&serde_json::json!({ "takeover": 1 })), None);
    }

    #[test]
    fn only_a_report_about_the_centre_is_the_suppressions() {
        assert_eq!(
            centre_report_from(&crate::liveness::unreachable_body("centre"), "centre"),
            Some(Report::Unreachable("centre".to_string()))
        );
        assert_eq!(
            centre_report_from(&crate::liveness::reachable_body("centre"), "centre"),
            Some(Report::Reachable("centre".to_string()))
        );
        // A report about another peer is that peer's business...
        assert_eq!(
            centre_report_from(&crate::liveness::unreachable_body("dev-b"), "centre"),
            None
        );
        // ...and nothing else reads as one.
        assert_eq!(
            centre_report_from(&crate::liveness::probe_body(), "centre"),
            None
        );
        assert_eq!(
            centre_report_from(&takeover_body("centre", "dev-a", T0), "centre"),
            None
        );
    }

    #[test]
    fn a_takeover_broadcast_stands_a_confirming_node_down() {
        let mut machine = Suppression::new("dev-a", T0);
        machine.observe(false, T0);
        machine.observe(false, T0 + WAIT_MS);
        assert_eq!(machine.phase(), SuppressionPhase::Confirming);
        // The broadcast arrives (v1.0 M5-3b-1): §33's third layer — and the machine is a candidate again.
        let heard =
            is_takeover(&takeover_body("centre", "dev-b", T0 + WAIT_MS)).expect("a broadcast");
        assert_eq!(heard.by, "dev-b");
        machine.stand_down(T0 + WAIT_MS + 1);
        assert_eq!(machine.phase(), SuppressionPhase::Candidate);
    }
}
