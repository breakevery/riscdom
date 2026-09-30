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

    /// **Stand in, if the backoff is over.** The caller has already checked that this node is
    /// **first in line** ([`is_first_in_line`] — precedence is a fact about the group, not about this
    /// machine); this is the other half, and it moves `BackingOff` → `StandingIn` **once**. Answers whether it
    /// did (v1.0 M5-3b-2).
    pub fn maybe_stand_in(&mut self, now: i64) -> bool {
        if self.phase != SuppressionPhase::BackingOff || !self.backoff_elapsed(now) {
            return false;
        }
        self.phase = SuppressionPhase::StandingIn;
        self.since_ms = now;
        self.deadline_ms = None;
        true
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

/// One event of a closed segment, on its way to the centre (v1.0 M5-3c-2).
///
/// A stand-in's events were written to its own main chain, and the centre merges them by transcription.
/// What travels is not the source row (its `id` and hashes belong to the sender's chain) but the five
/// fields that decide what the transcription writes: when, who, what, the detail, and the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentEvent {
    /// The segment it belongs to — the name it was opened under.
    pub segment_id: String,
    /// The centre it is being handed to.
    pub centre: String,
    /// Where it sits in the stream, from zero.
    pub index: usize,
    /// How many events the stream holds.
    pub total: usize,
    /// When the event was written on the sender's chain.
    pub ts: i64,
    /// The event's actor.
    pub actor: String,
    /// The event's action.
    pub action: String,
    /// The event's agent id, when it had one.
    pub agent_id: Option<String>,
    /// The event's detail, verbatim.
    pub detail: Value,
}

/// The end of a segment's stream (v1.0 M5-3c-2).
///
/// A stream needs an end, and this is it: the count is checked against what arrived, and the sender's
/// cross-segment reference travels with it so the centre can record the same anchor the segment was
/// opened with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentDone {
    /// The segment whose stream just ended.
    pub segment_id: String,
    /// The centre it was handed to.
    pub centre: String,
    /// How many events the sender sent.
    pub total: usize,
    /// The cross-segment reference the segment was opened with, when the sender had one.
    pub head_prev_chain: Option<String>,
    /// How long the sender's chain was at that reference (v1.0 M6-5-1). The anchor's second half: the
    /// hash says *which point* and this says *how far along*, so a centre holding a digest of its own
    /// for that sender can compare rather than guess (M6-5-2 does the comparing). `None` when the sender
    /// opened the segment before it recorded a length.
    pub anchor_length: Option<u64>,
}

/// The body one segment event travels as (v1.0 M5-3c-2).
///
/// An ordinary §3 frame's body, like §6.6's registration and §6.7's reports — one event per frame,
/// because a frame has a size ceiling ([`DEFAULT_MAX_FRAME_BYTES`](crate::DEFAULT_MAX_FRAME_BYTES))
/// and a segment may hold more than one frame's worth.
pub fn segment_event_body(event: &SegmentEvent) -> Value {
    serde_json::json!({
        "segment_event": PROTOCOL_VERSION,
        "segment_id": event.segment_id,
        "centre": event.centre,
        "index": event.index,
        "total": event.total,
        "ts": event.ts,
        "actor": event.actor,
        "action": event.action,
        "agent_id": event.agent_id,
        "detail": event.detail,
    })
}

/// Read a segment event out of a body, when the body is one.
///
/// Every field is required: a stream frame that cannot be rebuilt into an event is not a frame this
/// node should act on, and `None` says so rather than half an event.
pub fn is_segment_event(body: &Value) -> Option<SegmentEvent> {
    if body.get("segment_event").and_then(Value::as_u64) != Some(u64::from(PROTOCOL_VERSION)) {
        return None;
    }
    Some(SegmentEvent {
        segment_id: body.get("segment_id")?.as_str()?.to_string(),
        centre: body.get("centre")?.as_str()?.to_string(),
        index: body.get("index")?.as_u64()? as usize,
        total: body.get("total")?.as_u64()? as usize,
        ts: body.get("ts")?.as_i64()?,
        actor: body.get("actor")?.as_str()?.to_string(),
        action: body.get("action")?.as_str()?.to_string(),
        agent_id: body
            .get("agent_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        detail: body.get("detail")?.clone(),
    })
}

/// Why a delivered segment did not pass the **envelope** check (v1.0 M6-5-2a).
///
/// This is about the *delivery* and not about the sender's chain: a stream is a run of [`SegmentEvent`]s
/// and one [`SegmentDone`], and these are the ways the two can disagree with each other or with the node
/// that was addressed. It says nothing about whether those events were really on the sender's chain —
/// that would need the events' own hashes, which do not travel (M6-5-2b).
///
/// It lives here rather than in `audit` because [`SegmentEvent`] and [`SegmentDone`] are `net`'s own
/// shapes: `audit` is a leaf the connection layer depends on, and it must not learn about frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryProblem {
    /// The end frame was addressed to a node that is not this one.
    NotForThisNode { said: String, this_node: String },
    /// The count the end frame claims and the events that arrived disagree.
    Count { said: usize, arrived: usize },
    /// The events' positions are not exactly `0..total` once each: a gap, or a repeat.
    Positions { expected: usize, found: Vec<usize> },
}

impl DeliveryProblem {
    /// One sentence, for an event's detail and for a log line.
    pub fn reason(&self) -> String {
        match self {
            DeliveryProblem::NotForThisNode { said, this_node } => {
                format!("the stream was addressed to {said}, and this node is {this_node}")
            }
            DeliveryProblem::Count { said, arrived } => {
                format!("the end frame says {said} event(s) and {arrived} arrived")
            }
            DeliveryProblem::Positions { expected, found } => {
                format!("the events are not the positions 0..{expected}: {found:?}")
            }
        }
    }
}

/// Check a delivered segment's **envelope** against the end frame that closed it (v1.0 M6-5-2a).
///
/// Pure, so the check can be read without a socket and the caller decides what an answer means. Three
/// things, and they are all the delivery itself can be asked:
///
/// - the stream was addressed to **this node**;
/// - as many events arrived as the end frame says;
/// - the events' positions are **exactly `0..total`**, each once — compared as a set, because the stream
///   says the events arrive in order and the receiver sorts them before transcribing, so what this
///   proves is that every position arrived exactly once.
///
/// A stream that carried **no** event is consistent when `total` is `0` and inconsistent when it is not —
/// which is how an empty segment stops being silently dropped (M5-3c-2 discarded it) without a
/// legitimate empty segment being refused.
pub fn verify_delivery(
    this_node: &str,
    done: &SegmentDone,
    events: &[SegmentEvent],
) -> Result<(), DeliveryProblem> {
    if done.centre != this_node {
        return Err(DeliveryProblem::NotForThisNode {
            said: done.centre.clone(),
            this_node: this_node.to_string(),
        });
    }
    if events.len() != done.total {
        return Err(DeliveryProblem::Count {
            said: done.total,
            arrived: events.len(),
        });
    }
    let mut found: Vec<usize> = events.iter().map(|event| event.index).collect();
    found.sort_unstable();
    if found != (0..done.total).collect::<Vec<_>>() {
        return Err(DeliveryProblem::Positions {
            expected: done.total,
            found,
        });
    }
    Ok(())
}

/// The body a segment stream's end travels as (v1.0 M5-3c-2).
pub fn segment_done_body(done: &SegmentDone) -> Value {
    // `head_prev_chain` and `anchor_digest` are **one value under two names** (v1.0 M6-5-1): the field
    // has kept its M5-3c-2 name so every existing reader keeps working, and the new name is the anchor
    // vocabulary's own. Both are written from the same struct field, so they cannot drift.
    serde_json::json!({
        "segment_done": PROTOCOL_VERSION,
        "segment_id": done.segment_id,
        "centre": done.centre,
        "total": done.total,
        "head_prev_chain": done.head_prev_chain,
        "anchor_digest": done.head_prev_chain,
        "anchor_length": done.anchor_length,
    })
}

/// Read a segment stream's end out of a body, when the body is one.
pub fn is_segment_done(body: &Value) -> Option<SegmentDone> {
    if body.get("segment_done").and_then(Value::as_u64) != Some(u64::from(PROTOCOL_VERSION)) {
        return None;
    }
    Some(SegmentDone {
        segment_id: body.get("segment_id")?.as_str()?.to_string(),
        centre: body.get("centre")?.as_str()?.to_string(),
        total: body.get("total")?.as_u64()? as usize,
        // The anchor digest is read under its new name first and falls back to the M5-3c-2 name, so a
        // segment from a sender that predates M6-5-1 still reads (v1.0 M6-5-1). Both spellings are a
        // string or nothing: `null` on an empty chain is "no anchor", not an error.
        head_prev_chain: body
            .get("anchor_digest")
            .and_then(Value::as_str)
            .or_else(|| body.get("head_prev_chain").and_then(Value::as_str))
            .map(str::to_string),
        // The length is optional and lenient: absent or not a number means "the sender did not say",
        // which is the honest reading for a segment opened before the length was recorded.
        anchor_length: body.get("anchor_length").and_then(Value::as_u64),
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
        // M5-3a stops at the backoff: `StandingIn` is M5-3b's act, and nothing there reaches it.
        let mut machine = Suppression::new("dev-a", T0);
        machine.observe(false, T0);
        machine.observe(false, T0 + WAIT_MS);
        let table = WitnessTable::new();
        table.record("dev-b", "centre", false, T0 + WAIT_MS);
        machine
            .confirm(&table, "centre", T0 + WAIT_MS, |_| true)
            .expect("judged");
        assert_eq!(machine.phase(), SuppressionPhase::BackingOff);
        // Now M5-3b-2's half: only past the deadline, and only once.
        let deadline = machine.deadline_ms().expect("a deadline");
        assert!(!machine.maybe_stand_in(deadline - 1), "not yet");
        assert_eq!(machine.phase(), SuppressionPhase::BackingOff);
        assert!(machine.maybe_stand_in(deadline), "past the backoff");
        assert_eq!(machine.phase(), SuppressionPhase::StandingIn);
        assert!(!machine.maybe_stand_in(deadline + 1), "once");
        assert_eq!(SuppressionPhase::StandingIn.as_str(), "standing-in");
    }

    #[test]
    fn standing_in_is_not_reached_from_a_wait() {
        let mut machine = Suppression::new("dev-a", T0);
        // No confirm yet: a wait cannot be skipped into a takeover.
        assert!(!machine.maybe_stand_in(T0 + WAIT_MS * 10));
        assert_eq!(machine.phase(), SuppressionPhase::Candidate);
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

    /// The end of a segment's stream carries the whole anchor, and an older sender still reads
    /// (v1.0 M6-5-1).
    #[test]
    fn a_segment_end_frame_carries_the_anchor_and_tolerates_an_older_sender() {
        let done = SegmentDone {
            segment_id: "seg-dev-a-1".to_string(),
            centre: "centre".to_string(),
            total: 3,
            head_prev_chain: Some("abc123".to_string()),
            anchor_length: Some(9),
        };
        let body = segment_done_body(&done);
        // The digest travels under both of its names, and behind them is one value.
        assert_eq!(body["head_prev_chain"], serde_json::json!("abc123"));
        assert_eq!(body["anchor_digest"], serde_json::json!("abc123"));
        assert_eq!(body["anchor_length"], serde_json::json!(9));
        assert_eq!(is_segment_done(&body), Some(done));

        // A frame from a sender that predates the anchor names: the M5-3c-2 spelling, read as one.
        let older = serde_json::json!({
            "segment_done": 1,
            "segment_id": "seg-dev-a-2",
            "centre": "centre",
            "total": 0,
            "head_prev_chain": "def456",
        });
        assert_eq!(
            is_segment_done(&older),
            Some(SegmentDone {
                segment_id: "seg-dev-a-2".to_string(),
                centre: "centre".to_string(),
                total: 0,
                head_prev_chain: Some("def456".to_string()),
                anchor_length: None,
            }),
            "a sender that predates M6-5-1 still reads"
        );

        // A `null` anchor is "no anchor", and a length that is not a number is not a length.
        let empty = serde_json::json!({
            "segment_done": 1,
            "segment_id": "seg-dev-a-3",
            "centre": "centre",
            "total": 2,
            "head_prev_chain": null,
            "anchor_digest": null,
            "anchor_length": "seven",
        });
        let read = is_segment_done(&empty).expect("a done frame");
        assert_eq!(read.head_prev_chain, None);
        assert_eq!(read.anchor_length, None);
    }

    /// The delivery check reads the envelope and nothing else (v1.0 M6-5-2a).
    #[test]
    fn a_delivery_is_checked_against_its_own_end_frame() {
        let done = SegmentDone {
            segment_id: "seg-dev-a-1".to_string(),
            centre: "centre".to_string(),
            total: 2,
            head_prev_chain: Some("abc".to_string()),
            anchor_length: Some(5),
        };
        let event = |index: usize| SegmentEvent {
            segment_id: "seg-dev-a-1".to_string(),
            centre: "centre".to_string(),
            index,
            total: 2,
            ts: T0,
            actor: "host".to_string(),
            action: "host.test.x".to_string(),
            agent_id: None,
            detail: serde_json::json!({ "n": index }),
        };

        // Both positions, once each: it passes, in either arrival order.
        assert_eq!(
            verify_delivery("centre", &done, &[event(0), event(1)]),
            Ok(())
        );
        assert_eq!(
            verify_delivery("centre", &done, &[event(1), event(0)]),
            Ok(())
        );

        // A stream that ended without an event is consistent when the end frame says so...
        let empty = SegmentDone {
            total: 0,
            ..done.clone()
        };
        assert_eq!(verify_delivery("centre", &empty, &[]), Ok(()));
        // ...and a gap when it does not.
        assert_eq!(
            verify_delivery("centre", &done, &[]),
            Err(DeliveryProblem::Count {
                said: 2,
                arrived: 0
            })
        );

        // A repeat is as wrong as a gap.
        assert_eq!(
            verify_delivery("centre", &done, &[event(1), event(1)]),
            Err(DeliveryProblem::Positions {
                expected: 2,
                found: vec![1, 1]
            })
        );

        // One too many: the count catches it before the positions do.
        assert_eq!(
            verify_delivery("centre", &done, &[event(0), event(1), event(1)]),
            Err(DeliveryProblem::Count {
                said: 2,
                arrived: 3
            })
        );

        // A stream addressed elsewhere is refused first, and says who it was for.
        let elsewhere = DeliveryProblem::NotForThisNode {
            said: "centre".to_string(),
            this_node: "dev-me".to_string(),
        };
        assert_eq!(
            verify_delivery("dev-me", &done, &[event(0), event(1)]),
            Err(elsewhere.clone())
        );
        assert!(elsewhere.reason().contains("dev-me"), "{elsewhere:?}");
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
