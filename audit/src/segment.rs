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
    /// When it opened.
    pub opened_at_ms: Option<i64>,
    /// When it closed. `None` while it is open.
    pub closed_at_ms: Option<i64>,
    /// Where it is in its life.
    pub state: SegmentState,
    /// A human note. Free-form, and stored beside the chain like everything else here.
    pub note: Option<String>,
}
