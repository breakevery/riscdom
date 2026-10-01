//! The segment record (v1.0 M5-1a).
//!
//! [roadmap §7](../../docs/roadmap-v1.0.md) and [decisions §127](../../docs/decisions.md) extend the
//! chain's *semantics* to "main chain + temporary segments" without moving its formula. This module is
//! the **data shape** of that split: what a segment is, which kind it is, and where it is in its life.
//!
//! **It connects to no behaviour yet.** M5-1a lands the shape and the table it is stored in
//! (`segments`); M5-1b is what opens and closes one, and M5-2 is what folds or forks it. The full
//! semantics — and the three questions roadmap §7 still leaves open — are written down in
//! [docs/audit-v2.md](../../docs/audit-v2.md).

use serde::{Deserialize, Serialize};

/// The audit event that records a segment **opening** (v1.0 M5-1b).
///
/// It is written on the **main chain** (`segment_id IS NULL`): it describes the segment's *life*, and
/// is not one of the segment's own events. Same for [`ACTION_SEGMENT_CLOSED`].
pub const ACTION_SEGMENT_OPENED: &str = "host.audit.segment_opened";

/// The audit event that records a segment **closing** (v1.0 M5-1b).
pub const ACTION_SEGMENT_CLOSED: &str = "host.audit.segment_closed";

/// The audit event that records a segment's events being **merged** into the main chain (v1.0 M5-2a).
///
/// Reserved by [docs/audit-v2.md](../../docs/audit-v2.md) §6 in M5-1a and written by the merge in M5-2a.
pub const ACTION_SEGMENT_MERGED: &str = "host.audit.segment_merged";

/// The audit event that records a **conflict** found while merging a segment (v1.0 M5-2b).
///
/// Reserved in [docs/audit-v2.md](../../docs/audit-v2.md) §6 and written by the merge when it meets an act the
/// main chain already holds. It is the **mark** on the chain side: the segment keeps its own file, and the
/// main chain records that the two disagree.
pub const ACTION_SEGMENT_FORKED: &str = "host.audit.segment_forked";

/// The audit event that records a delivered segment **passing** the checks its receiver can make
/// (v1.0 M6-5-2a).
///
/// The checks are the delivery's own: the events match the end frame, and the store the receiver rebuilt
/// from them is intact. A segment whose anchor carries no length predates that column, so its row is
/// written with `checked: "skipped"` rather than pretending it was verified. What the receiver **cannot**
/// check yet — that those events were really on the sender's chain — needs the events' own hashes and is
/// M6-5-2b's.
///
/// Like the other `host.audit.*` names this is an **audit event name**, not a stream name: the twenty
/// `control-plane-events.md` names are a different vocabulary.
pub const ACTION_CHAIN_VERIFIED: &str = "host.audit.chain_verified";

/// The audit event that records a delivered segment **failing** those checks (v1.0 M6-5-2a).
///
/// It replaces silence. The segment is **not** merged — nothing of it reaches the main chain — and it is
/// not marked `forked` either: a fork is "both sides are real and nobody has said which is right", and
/// this is "the delivery did not hold together". The segment's own file is kept: a refused delivery is
/// still evidence.
pub const ACTION_CHAIN_REJECTED: &str = "host.audit.chain_rejected";

/// The audit event that records a **person deciding what to do about a conflict** (v1.0 M6-5-3b).
///
/// A fork is two acts that contradict each other, and the kernel does **not** choose between them (roadmap
/// §1: a rule would be policy). What the kernel can do is record that somebody decided — which is this
/// event, and nothing more: the segment row keeps `state = forked` (it means *unresolved*, and it is the
/// index; the chain is the record), nothing is transcribed, and no side is named as right.
///
/// Like the other `host.audit.*` names this is an **audit event name**, not a stream name: the twenty
/// `control-plane-events.md` names are a different vocabulary.
pub const ACTION_CONFLICT_RESOLVED: &str = "host.audit.conflict_resolved";

/// The prefix a **partial merge** leaves in a segment's `note` (v1.0 M5-2a).
///
/// A merge that failed part way through cannot be undone (the chain only grows), so it records what happened
/// here. [`partial_merge_recorded`] is what later reads it: a retry must not mistake its own half-written
/// events for a conflict.
pub const PARTIAL_MERGE_PREFIX: &str = "merge failed after ";

/// Whether a segment's `note` records a partial merge.
pub fn partial_merge_recorded(note: &str) -> bool {
    note.starts_with(PARTIAL_MERGE_PREFIX)
}

/// The name of the detail member that marks an event as written during a temporary centre (decisions
/// §33/§127): `detail.provisional = true`.
pub const PROVISIONAL: &str = "provisional";

/// The **cleared** form of an event's detail: the same object with `provisional` removed (v1.0 M5-2a).
///
/// This is what "the mark is cleared" means when the mark is cleared by a merge: the transcribed event is
/// written **without** it. Nothing already on the chain is rewritten — the segment's own file keeps its
/// rows, `provisional` and all (decisions §127 point 3; the chain is append-only).
pub fn cleared_detail(detail: serde_json::Value) -> serde_json::Value {
    match detail {
        serde_json::Value::Object(mut map) => {
            map.remove(PROVISIONAL);
            serde_json::Value::Object(map)
        }
        other => other,
    }
}

/// Whether an event's detail carries the `provisional` mark.
pub fn is_provisional(detail: &serde_json::Value) -> bool {
    detail.get(PROVISIONAL).and_then(serde_json::Value::as_bool) == Some(true)
}

/// Which chain a segment belongs to ([docs/audit-v2.md](../../docs/audit-v2.md)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentKind {
    /// The main chain: what the device writes when no temporary centre is in force.
    Main,
    /// A temporary segment: what a stand-in centre writes while the real one is unreachable.
    Temporary,
}

impl SegmentKind {
    /// The word stored in the `segments` table.
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentKind::Main => "main",
            SegmentKind::Temporary => "temporary",
        }
    }

    /// Read the word back, staying honest about anything else.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "main" => Some(SegmentKind::Main),
            "temporary" => Some(SegmentKind::Temporary),
            _ => None,
        }
    }
}

/// Where a segment is in its life (decisions §33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SegmentState {
    /// Events are being appended to it.
    Open,
    /// It ended and was left as written.
    Closed,
    /// A conflict-free run folded into the chain that follows (§33's option A).
    Folded,
    /// A conflicting run: both sides stay marked (never a silent merge).
    Forked,
}

impl SegmentState {
    /// The word stored in the `segments` table.
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentState::Open => "open",
            SegmentState::Closed => "closed",
            SegmentState::Folded => "folded",
            SegmentState::Forked => "forked",
        }
    }

    /// Read the word back, staying honest about anything else.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "open" => Some(SegmentState::Open),
            "closed" => Some(SegmentState::Closed),
            "folded" => Some(SegmentState::Folded),
            "forked" => Some(SegmentState::Forked),
            _ => None,
        }
    }
}

/// One segment, as the `segments` table holds it (v1.0 M5-1a).
///
/// **Not part of the hash chain.** Nothing here is an input to
/// [`compute_hash`](crate::compute_hash): the cross-segment reference at a segment's head is **added
/// metadata**, which is what lets the chain's formula stay where it is (decisions §127 point 2). M5-1a
/// writes no row of this type; the shape is here so M5-1b has one place to write it down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    /// The segment's own name (the `segment_id` the events carry).
    pub segment_id: String,
    /// Which chain it belongs to.
    pub kind: SegmentKind,
    /// The segment's last hash, once it has events. `None` until then.
    pub head_hash: Option<String>,
    /// The **cross-segment reference**: the main chain's head when the segment opened. A plain field,
    /// never an input to the hash.
    pub head_prev_chain: Option<String>,
    /// The **length** of the chain that head was read from — the last event id at that moment (v1.0
    /// M6-5-1). The anchor's second half: a hash names a point, and this says *which* point, so a
    /// verifier can compare a digest it has rather than only a head it can never hold (M6-5-2 does the
    /// comparing; this batch only records the evidence). `None` for a row written before the column
    /// existed — the honest state, exactly as `agent_id` is `None` on a pre-v0.8 row.
    pub head_prev_length: Option<i64>,
    /// When it opened.
    pub opened_at_ms: Option<i64>,
    /// When it closed. `None` while it is open.
    pub closed_at_ms: Option<i64>,
    /// Where it is in its life.
    pub state: SegmentState,
    /// A human note. Free-form, and stored beside the chain like everything else here.
    pub note: Option<String>,
}

impl Segment {
    /// Read a row of the `segments` table (§3 of [docs/audit-v2.md](../../docs/audit-v2.md)).
    ///
    /// A `kind` or `state` word this build does not know is an **error**, not a guess: the table is
    /// either written by this code or it is not a table this code should read.
    pub(crate) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        fn bad(column: usize, value: String) -> rusqlite::Error {
            rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown segment word {value:?}"),
                )),
            )
        }
        let kind_word: String = row.get(1)?;
        let state_word: String = row.get(7)?;
        let kind = SegmentKind::parse(&kind_word).ok_or_else(|| bad(1, kind_word))?;
        let state = SegmentState::parse(&state_word).ok_or_else(|| bad(7, state_word))?;
        Ok(Self {
            segment_id: row.get(0)?,
            kind,
            head_hash: row.get(2)?,
            head_prev_chain: row.get(3)?,
            head_prev_length: row.get(4)?,
            opened_at_ms: row.get(5)?,
            closed_at_ms: row.get(6)?,
            state,
            note: row.get(8)?,
        })
    }
}

/// The detail of a [`ACTION_SEGMENT_OPENED`] event (v1.0 M5-1b): what opened, and the chain position it
/// continues from.
pub fn segment_opened_detail(segment: &Segment) -> serde_json::Value {
    serde_json::json!({
        "segment_id": segment.segment_id,
        "kind": segment.kind.as_str(),
        "head_prev_chain": segment.head_prev_chain,
    })
}

/// The part of a segment id that names its owner, made safe for a file name (v1.0 M5-3c-2).
///
/// A `node_id` is a deployer's own name and a name is free-form, while a segment id becomes a path
/// (`audit-segments/<segment_id>.db`, [`segment_db_path_in`](crate::segment_db_path_in)). So: ASCII letters,
/// digits, `-`, `_` and `.` are kept, every other character becomes `_` (which covers the separators, so
/// nothing here can steer the path), the result is capped at 48 characters, trailing dots go, and a name
/// that leaves nothing usable answers `node`.
pub fn safe_owner(owner: &str) -> String {
    let mut out = String::new();
    for ch in owner.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('_');
        }
        if out.chars().count() >= OWNER_MAX_CHARS {
            break;
        }
    }
    while out.ends_with('.') {
        out.pop();
    }
    if out.is_empty() {
        "node".to_string()
    } else {
        out
    }
}

/// How much of an owner's name a segment id keeps (see [`safe_owner`]).
const OWNER_MAX_CHARS: usize = 48;

/// What a merge did (v1.0 M5-2a; M5-2b added the fork).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeOutcome {
    /// The segment's events were transcribed onto the main chain and its row is `folded`.
    Folded {
        /// How many events were transcribed.
        merged: usize,
    },
    /// A conflict was found: nothing was transcribed, the row is `forked`, and the chain records it.
    Forked {
        /// Why, in words a person can read.
        reason: String,
    },
}

/// The detail of a [`ACTION_SEGMENT_CLOSED`] event (v1.0 M5-1b): what closed, and when.
pub fn segment_closed_detail(segment_id: &str, closed_at_ms: i64) -> serde_json::Value {
    serde_json::json!({
        "segment_id": segment_id,
        "closed_at_ms": closed_at_ms,
    })
}

/// The detail of a [`ACTION_SEGMENT_FORKED`] event (v1.0 M5-2b): which segment, why, and what it clashed with.
pub fn segment_forked_detail(
    segment_id: &str,
    kind: SegmentKind,
    forked_at_ms: i64,
    reason: &str,
    conflicting_event_id: Option<i64>,
) -> serde_json::Value {
    serde_json::json!({
        "segment_id": segment_id,
        "kind": kind.as_str(),
        "forked_at_ms": forked_at_ms,
        "reason": reason,
        "conflicting_event_id": conflicting_event_id,
    })
}

/// The detail of a [`ACTION_CHAIN_VERIFIED`] event (v1.0 M6-5-2a; the chaining word added in M6-5-2b): the
/// delivery, and what was checked.
///
/// Two words, because two different things can be true at once:
///
/// - **`checked`** — the furthest check that was *attempted*: `"skipped"` (nothing), `"delivery"` (the
///   envelope and the rebuild), or `"delivery+chained"` (those **and** the stream's own hashes recomputed
///   and linked).
/// - **`linkage`** — the chaining verdict: `"ok"`, `"broken"`, or `"skipped"` (not attempted, or nothing
///   to chain).
///
/// Neither word ever claims the anchor: tying the first event back to it is a **remaining goal**
/// ([cross-chain-verification.md](../../docs/cross-chain-verification.md)), because the link between them is
/// the `segment_opened` marker, which is not delivered.
/// The detail of a [`ACTION_CHAIN_VERIFIED`] event (v1.0 M6-5-2a; M6-5-2b and M6-5-4 added words): the
/// delivery, and the checks it survived.
#[allow(clippy::too_many_arguments)]
pub fn chain_verified_detail(
    segment_id: &str,
    from: &str,
    anchor_digest: Option<&str>,
    anchor_length: Option<i64>,
    events: usize,
    checked: &str,
    linkage: &str,
    // The anchor-link verdict (v1.0 M6-5-4): `"skipped"` (the sender named no opened-row hash), `"ok"`
    // (the first delivered event continues from it), `"broken"` (it does not).
    anchor: &str,
) -> serde_json::Value {
    serde_json::json!({
        "segment_id": segment_id,
        "from": from,
        "anchor_digest": anchor_digest,
        "anchor_length": anchor_length,
        "events": events,
        "checked": checked,
        "linkage": linkage,
        "anchor": anchor,
    })
}

/// The detail of a [`ACTION_CHAIN_REJECTED`] event (v1.0 M6-5-2a; the two words added in M6-5-2b): the
/// delivery, and why it was refused.
///
/// The arguments are the delivery's own facts, one per field of the detail: grouping them would be a second
/// description of something that already has one (the same reason `dispatch_task` keeps its flat list).
#[allow(clippy::too_many_arguments)]
pub fn chain_rejected_detail(
    segment_id: &str,
    from: &str,
    anchor_digest: Option<&str>,
    anchor_length: Option<i64>,
    events: usize,
    reason: &str,
    checked: &str,
    linkage: &str,
    // The anchor-link verdict (v1.0 M6-5-4): see [`chain_verified_detail`].
    anchor: &str,
) -> serde_json::Value {
    serde_json::json!({
        "segment_id": segment_id,
        "from": from,
        "anchor_digest": anchor_digest,
        "anchor_length": anchor_length,
        "events": events,
        "reason": reason,
        "checked": checked,
        "linkage": linkage,
        "anchor": anchor,
    })
}

/// The detail of a [`ACTION_CONFLICT_RESOLVED`] event (v1.0 M6-5-3b): who decided, when, and what they
/// noted.
///
/// `resolved_by` is the **caller's** identity (the request's actor), and `note` is the person's own words —
/// `null` when they wrote none. The two are deliberately the whole record: **which side is right is not
/// here**, because the kernel did not decide it.
pub fn conflict_resolved_detail(
    segment_id: &str,
    resolved_by: &str,
    resolved_at_ms: i64,
    note: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "segment_id": segment_id,
        "resolved_by": resolved_by,
        "resolved_at_ms": resolved_at_ms,
        "note": note,
    })
}

/// The detail of a [`ACTION_SEGMENT_MERGED`] event (v1.0 M5-2a): what was merged, and how much of it.
pub fn segment_merged_detail(
    segment_id: &str,
    kind: SegmentKind,
    merged_at_ms: i64,
    event_count: usize,
) -> serde_json::Value {
    serde_json::json!({
        "segment_id": segment_id,
        "kind": kind.as_str(),
        "merged_at_ms": merged_at_ms,
        "event_count": event_count,
    })
}
