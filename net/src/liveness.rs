//! Liveness: the probe, the prober's own view, and the collective judgement (v1.0 V-3a).
//!
//! [connection.md §6.7](../../docs/connection.md) freezes this. §6.6 gave every node a
//! *report*; §6.7 gives the deployment a *judgement* — a different fact, and the two must not
//! be confused. A row that has gone `offline` says **the server stopped hearing from a node**;
//! a **judgement** says **everybody who could still reach it has said they cannot**, which is
//! the only statement strong enough to act on.
//!
//! Three pieces, and they are three layers of one rule:
//!
//! - **The probe** ([`probe_body`] / [`alive_body`]): an ordinary §3 frame addressed to a peer,
//!   answered by the peer's **key-holder** — so a prober gets evidence about the node behind
//!   the socket, not merely that a socket is open.
//! - **The prober's own view** ([`Prober`]): per peer, when the last answer arrived, how many
//!   probes in a row went unanswered, and whether the peer is held *reachable* or
//!   *unreachable*. Memory-only, and written to no chain — a suspicion is not an event.
//! - **The judgement** ([`Judgement`], [`WitnessTable`]): the server's rule — unanimity among
//!   the witnesses that remain, with a witness of life vetoing.
//!
//! **Dependency direction.** Nothing here writes an audit row or names a capability: `net`
//! answers *who sent this* and *who is where*, and the chain is host-core's. The transition a
//! judgement produces is handed to a [`TransitionSink`] the deployment wires.

use crate::message::PROTOCOL_VERSION;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How often a node probes a peer — §6.6's heartbeat interval, on purpose (§6.7).
///
/// The numbers are §6.6's so that the node's own evidence and the server's table cross the
/// same line at the same moment; they are v1.0's defaults, of the kind §3.1's timeouts are.
pub const PROBE_INTERVAL: Duration = Duration::from_secs(15);

/// Three consecutive unanswered probes is 45 s (§6.7).
pub const PROBE_MISSES: u32 = 3;

/// How long a report counts: fresh inside the same 45 s (§6.7).
pub const REPORT_WINDOW_MS: i64 = 3 * 15_000;

/// The body a prober asks a peer with (§6.7).
pub fn probe_body() -> Value {
    serde_json::json!({ "probe": PROTOCOL_VERSION })
}

/// Is this body a probe?
pub fn is_probe(body: &Value) -> bool {
    body.get("probe").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// The body a peer answers a probe with: the smallest evidence that the node behind the socket
/// is working (§6.7).
pub fn alive_body() -> Value {
    serde_json::json!({ "alive": PROTOCOL_VERSION })
}

/// Is this body an answer to a probe?
pub fn is_alive(body: &Value) -> bool {
    body.get("alive").and_then(Value::as_u64) == Some(u64::from(PROTOCOL_VERSION))
}

/// The body a prober reports a peer unreachable with (§6.7).
pub fn unreachable_body(node_id: &str) -> Value {
    serde_json::json!({ "unreachable": node_id })
}

/// The body a prober reports a peer reachable with (§6.7).
pub fn reachable_body(node_id: &str) -> Value {
    serde_json::json!({ "reachable": node_id })
}

/// What a prober tells its server about one peer (§6.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// The prober cannot reach the peer.
    Unreachable(String),
    /// The prober can reach the peer again.
    Reachable(String),
}

impl Report {
    /// The peer the report is about.
    pub fn node_id(&self) -> &str {
        match self {
            Report::Unreachable(node_id) | Report::Reachable(node_id) => node_id,
        }
    }

    /// Is this a witness of life?
    pub fn reachable(&self) -> bool {
        matches!(self, Report::Reachable(_))
    }

    /// The body it travels as.
    pub fn to_body(&self) -> Value {
        match self {
            Report::Unreachable(node_id) => unreachable_body(node_id),
            Report::Reachable(node_id) => reachable_body(node_id),
        }
    }
}

/// Read a report out of a body (§6.7's two bodies, and nothing else).
pub fn report_of(body: &Value) -> Option<Report> {
    if let Some(node_id) = body.get("unreachable").and_then(Value::as_str) {
        return Some(Report::Unreachable(node_id.to_string()));
    }
    if let Some(node_id) = body.get("reachable").and_then(Value::as_str) {
        return Some(Report::Reachable(node_id.to_string()));
    }
    None
}

/// What a prober holds about one peer (§6.7): the last answer, the misses in a row, and whether
/// the peer is held reachable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerView {
    /// When the peer last answered a probe; `None` until the first answer.
    pub last_answer_ms: Option<i64>,
    /// How many probes in a row have gone unanswered.
    pub consecutive_misses: u32,
    /// What the prober currently holds.
    pub reachable: bool,
}

/// A prober's own view of its workgroup peers (§6.7).
///
/// It is **runtime state**: memory-only, like the server's table (§6.6) and §3.2's record, and
/// written to **no chain**, because a suspicion is not an event. A peer starts **reachable**
/// (a prober has no evidence against it until three misses), and the third miss in a row is
/// what turns it unreachable.
#[derive(Debug, Clone, Default)]
pub struct Prober {
    views: BTreeMap<String, PeerView>,
}

impl Prober {
    /// A prober watching `peers`, each held reachable.
    pub fn new<I, S>(peers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            views: peers
                .into_iter()
                .map(|peer| {
                    (
                        peer.into(),
                        PeerView {
                            last_answer_ms: None,
                            consecutive_misses: 0,
                            reachable: true,
                        },
                    )
                })
                .collect(),
        }
    }

    /// The peers, in order.
    pub fn peers(&self) -> Vec<String> {
        self.views.keys().cloned().collect()
    }

    /// Is there nobody to probe?
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// One peer's view.
    pub fn view(&self, peer: &str) -> Option<&PeerView> {
        self.views.get(peer)
    }

    /// A peer answered a probe: its misses reset, and the one transition an answer reports is
    /// a peer that had been held unreachable coming back (§6.7's `{"reachable": …}`).
    pub fn record_answer(&mut self, peer: &str, now: i64) -> Option<Report> {
        let view = self.views.get_mut(peer)?;
        view.last_answer_ms = Some(now);
        view.consecutive_misses = 0;
        if view.reachable {
            None
        } else {
            view.reachable = true;
            Some(Report::Reachable(peer.to_string()))
        }
    }

    /// A probe went unanswered this cycle: the miss count climbs, and the third in a row makes
    /// the peer unreachable (§6.7's `{"unreachable": …}`).
    pub fn record_miss(&mut self, peer: &str) -> Option<Report> {
        let view = self.views.get_mut(peer)?;
        view.consecutive_misses = view.consecutive_misses.saturating_add(1);
        if !view.reachable || view.consecutive_misses < PROBE_MISSES {
            return None;
        }
        view.reachable = false;
        Some(Report::Unreachable(peer.to_string()))
    }

    /// The reports to send **this** cycle: the prober's view for every peer, repeated each
    /// cycle while the view stands — §6.7's pulse, so a witness that goes quiet stops being one.
    pub fn pulse(&self) -> Vec<Report> {
        self.views
            .iter()
            .map(|(peer, view)| {
                if view.reachable {
                    Report::Reachable(peer.clone())
                } else {
                    Report::Unreachable(peer.clone())
                }
            })
            .collect()
    }
}

/// A judgement: the threshold first held for `peer` (§6.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgement {
    /// The node judged gone.
    pub peer: String,
    /// The witnesses that reported it unreachable — every witness that remained, which is what
    /// unanimity means.
    pub witnesses: Vec<String>,
    /// How many fresh reports backed the judgement (one per witness).
    pub reports: usize,
}

/// Which evidence ended a judgement (§6.7): the node's own beat, or a prober's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoverMethod {
    /// The node's own heartbeat was heard.
    Heartbeat,
    /// A prober answered (and reported it reachable).
    Probe,
}

/// One transition of the collective judgement (§6.7's two audit names, as a value).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transition {
    /// The threshold held: `host.connection.peer_offline`.
    Judged(Judgement),
    /// The node was heard from again: `host.connection.peer_recovered`.
    Recovered {
        /// The node that came back.
        peer: String,
        /// What was heard.
        method: RecoverMethod,
    },
}

/// Where a judgement goes: the deployment wires a sink, so the chain stays host-core's business
/// and `net` stays free of it.
pub type TransitionSink = Arc<dyn Fn(&Transition) + Send + Sync>;

/// The server's witness records: for each subject, who reported what and when (§6.7).
///
/// Runtime state, like §6.6's table: the server keeps it in memory and writes no chain of its
/// own — the two rows a judgement produces are written by whichever process runs the server and
/// holds a chain ([`TransitionSink`]).
#[derive(Clone, Default)]
pub struct WitnessTable {
    inner: Arc<WitnessInner>,
}

#[derive(Default)]
struct WitnessInner {
    /// subject `node_id` → witness `node_id` → the latest report.
    subjects: Mutex<HashMap<String, HashMap<String, WitnessReport>>>,
}

#[derive(Debug, Clone, Copy)]
struct WitnessReport {
    reachable: bool,
    at_ms: i64,
}

impl WitnessTable {
    /// Nobody has reported anything.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one witness's report about one subject. The latest report replaces an earlier one
    /// from the same witness: a report is a **view**, not a tally.
    pub fn record(&self, witness: &str, subject: &str, reachable: bool, now: i64) {
        let mut subjects = self
            .inner
            .subjects
            .lock()
            .expect("the witness table is not poisoned");
        subjects.entry(subject.to_string()).or_default().insert(
            witness.to_string(),
            WitnessReport {
                reachable,
                at_ms: now,
            },
        );
    }

    /// The judgement for `subject` at `now`, or none — §6.7's rule.
    ///
    /// A witness counts only while it is **fresh** (inside [`REPORT_WINDOW_MS`]), **itself
    /// reachable right now** (`witness_online`), and **not the subject**. A fresh witness of
    /// life **vetoes**; otherwise the subject is judged gone when at least one such witness
    /// remains and every one of them reported it unreachable — unanimity among those still able
    /// to speak.
    pub fn judge(
        &self,
        subject: &str,
        now: i64,
        witness_online: impl Fn(&str) -> bool,
    ) -> Option<Judgement> {
        let reports = {
            let subjects = self
                .inner
                .subjects
                .lock()
                .expect("the witness table is not poisoned");
            subjects.get(subject)?.clone()
        };
        let mut witnesses: BTreeSet<String> = BTreeSet::new();
        let mut veto = false;
        for (witness, report) in &reports {
            if witness == subject {
                continue;
            }
            if now - report.at_ms > REPORT_WINDOW_MS {
                continue;
            }
            if !witness_online(witness) {
                continue;
            }
            if report.reachable {
                veto = true;
            }
            witnesses.insert(witness.clone());
        }
        if veto || witnesses.is_empty() {
            return None;
        }
        Some(Judgement {
            peer: subject.to_string(),
            reports: witnesses.len(),
            witnesses: witnesses.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: i64 = 1_700_000_000_000;

    #[test]
    fn a_peer_is_held_reachable_until_three_misses() {
        let mut prober = Prober::new(["dev-b"]);
        // The first two misses are not enough; the third makes it unreachable, once.
        assert_eq!(prober.record_miss("dev-b"), None);
        assert_eq!(prober.record_miss("dev-b"), None);
        assert_eq!(
            prober.record_miss("dev-b"),
            Some(Report::Unreachable("dev-b".to_string()))
        );
        assert_eq!(prober.record_miss("dev-b"), None, "already unreachable");
        assert!(!prober.view("dev-b").expect("a view").reachable);

        // An answer resets the view and reports the recovery once.
        assert_eq!(
            prober.record_answer("dev-b", T0),
            Some(Report::Reachable("dev-b".to_string()))
        );
        assert_eq!(prober.record_answer("dev-b", T0 + 1), None);
        let view = prober.view("dev-b").expect("a view");
        assert!(view.reachable);
        assert_eq!(view.consecutive_misses, 0);
        assert_eq!(view.last_answer_ms, Some(T0 + 1));
    }

    #[test]
    fn a_pulse_repeats_the_view_for_every_peer() {
        let mut prober = Prober::new(["dev-a", "dev-b"]);
        assert_eq!(
            prober.pulse(),
            vec![
                Report::Reachable("dev-a".to_string()),
                Report::Reachable("dev-b".to_string()),
            ]
        );
        prober.record_miss("dev-b");
        prober.record_miss("dev-b");
        prober.record_miss("dev-b");
        assert_eq!(
            prober.pulse(),
            vec![
                Report::Reachable("dev-a".to_string()),
                Report::Unreachable("dev-b".to_string()),
            ]
        );
    }

    #[test]
    fn the_report_bodies_round_trip_and_mean_what_they_say() {
        assert_eq!(
            report_of(&unreachable_body("dev-b")),
            Some(Report::Unreachable("dev-b".to_string()))
        );
        assert_eq!(
            report_of(&reachable_body("dev-b")),
            Some(Report::Reachable("dev-b".to_string()))
        );
        assert_eq!(report_of(&probe_body()), None);
        assert_eq!(report_of(&alive_body()), None);
        assert!(is_probe(&probe_body()) && is_alive(&alive_body()));
        assert!(!is_probe(&alive_body()) && !is_alive(&probe_body()));
    }

    #[test]
    fn unanimity_among_the_witnesses_that_remain() {
        let table = WitnessTable::new();
        table.record("dev-a", "dev-b", false, T0);
        // One witness, fresh, online, and reporting unreachable: judged.
        let judged = table
            .judge("dev-b", T0 + 1_000, |w| w == "dev-a")
            .expect("judged");
        assert_eq!(judged.peer, "dev-b");
        assert_eq!(judged.witnesses, vec!["dev-a".to_string()]);
        assert_eq!(judged.reports, 1);

        // A witness of life vetoes.
        table.record("dev-c", "dev-b", true, T0);
        assert_eq!(table.judge("dev-b", T0 + 1_000, |_| true), None);

        // A witness that is not itself reachable cannot testify.
        let solo = WitnessTable::new();
        solo.record("dev-a", "dev-b", false, T0);
        assert_eq!(solo.judge("dev-b", T0 + 1_000, |_| false), None);

        // And a stale report has stopped being a witness.
        assert_eq!(
            solo.judge("dev-b", T0 + REPORT_WINDOW_MS + 1, |_| true),
            None
        );
    }

    #[test]
    fn the_subject_never_testifies_about_itself() {
        let table = WitnessTable::new();
        table.record("dev-b", "dev-b", false, T0);
        assert_eq!(table.judge("dev-b", T0, |_| true), None);
    }
}
