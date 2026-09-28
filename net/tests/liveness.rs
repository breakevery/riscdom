//! Liveness (§6.7): the prober's reports and the server's collective judgement (v1.0 V-3a).
//!
//! [connection.md §6.7](../../docs/connection.md) freezes the rule; these check the server's half
//! of it. The four that matter most are the ones a careless reading gets wrong: **unanimity
//! among the witnesses that remain**, a **witness of life vetoing**, a **stale report no longer
//! being a witness**, and **recovery being heard from** rather than re-admitted.
//!
//! The judgement is driven with an explicit `now` (`route` / `answer_local` take it), so the
//! 45-second windows are a rule a test ages without waiting them out.

use net::{
    alive_body, is_alive, is_probe, probe_body, reachable_body, report_of, unreachable_body,
    Judgement, Local, LocalReply, NodeKey, Online, PeerEntry, PeersFile, RecoverMethod,
    Registration, RelayError, RelayServer, ReplayGuard, RoomsFile, SignedMessage, Transition,
    TransportConfig, REPORT_WINDOW_MS,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const T0: i64 = 1_700_000_000_000;

fn config() -> TransportConfig {
    TransportConfig {
        connect_timeout: Duration::from_secs(2),
        read_timeout: Duration::from_secs(2),
        write_timeout: Duration::from_secs(2),
        ..TransportConfig::default()
    }
}

fn entry(node_id: &str, key: &NodeKey) -> PeerEntry {
    PeerEntry::new(node_id, "127.0.0.1:1", key.public_jwk())
}

/// A server that knows the given peers, with a sink that records every transition.
fn server_with_sink(keys: &[(&str, &NodeKey)]) -> (RelayServer, Arc<Mutex<Vec<Transition>>>) {
    let server_key = NodeKey::generate().expect("key");
    let mut peers = PeersFile::empty();
    for (node_id, key) in keys {
        peers.peers.push(entry(node_id, key));
    }
    let server = RelayServer::new("server", server_key, peers, RoomsFile::empty(), config())
        .expect("server");
    let seen: Arc<Mutex<Vec<Transition>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    server.set_transition_sink(Arc::new(move |transition| {
        sink.lock().expect("seen").push(transition.clone());
    }));
    (server, seen)
}

/// Sign `body` from `from` at `ts`, route it, and let the server handle it.
fn send(
    server: &RelayServer,
    key: &NodeKey,
    from: &str,
    ts: i64,
    body: serde_json::Value,
) -> (Local, Result<LocalReply, RelayError>) {
    let message = SignedMessage::sign(key, from, "server", ts, body).expect("sign");
    let frame = message.to_line().expect("line");
    let routed = server.route(&frame, ts).expect("routed");
    let action = routed.local().expect("a local frame").clone();
    let reply = server.answer_local(routed.from(), &action, ts);
    (action, reply)
}

/// A frame whose answer this test does not read.
fn tell(server: &RelayServer, key: &NodeKey, from: &str, ts: i64, body: serde_json::Value) {
    let _ = send(server, key, from, ts, body);
}

/// Take a registration. The row is created **before** the acknowledgement is written, so the
/// reply is an error here (this server has no session for the node) and the row exists anyway.
fn register(server: &RelayServer, key: &NodeKey, from: &str, ts: i64) {
    let (action, _reply) = send(server, key, from, ts, net::register_body(&[], &[], &[]));
    assert!(matches!(action, Local::Register(_)), "{from} registered");
    assert!(
        server.online_at(ts).iter().any(|row| row.node_id == from),
        "the row for {from} exists"
    );
}

fn transitions(seen: &Arc<Mutex<Vec<Transition>>>) -> Vec<Transition> {
    seen.lock().expect("seen").clone()
}

fn row(server: &RelayServer, node_id: &str, now: i64) -> net::OnlineEntry {
    server
        .online_at(now)
        .into_iter()
        .find(|row| row.node_id == node_id)
        .unwrap_or_else(|| panic!("no row for {node_id}"))
}

#[test]
fn a_unanimous_report_judges_a_node_gone() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let (server, seen) = server_with_sink(&[("dev-a", &a), ("dev-b", &b)]);
    register(&server, &a, "dev-a", T0);
    register(&server, &b, "dev-b", T0);

    // dev-a's report is read as one of §6.7's two bodies.
    let (action, reply) = send(&server, &a, "dev-a", T0 + 1_000, unreachable_body("dev-b"));
    assert_eq!(
        action,
        Local::UnreachableReport {
            node_id: "dev-b".to_string()
        }
    );
    assert!(
        matches!(reply, Ok(LocalReply::UnreachableReported { .. })),
        "{reply:?}"
    );

    // The judgement holds: the row gains `judged_at_ms`, kept apart from `state`.
    let row = row(&server, "dev-b", T0 + 1_000);
    assert_eq!(row.judged_at_ms, Some(T0 + 1_000));
    assert_eq!(
        row.state,
        Online::Online,
        "the heartbeat window is separate"
    );

    assert_eq!(
        transitions(&seen),
        vec![Transition::Judged(Judgement {
            peer: "dev-b".to_string(),
            witnesses: vec!["dev-a".to_string()],
            reports: 1,
        })]
    );
}

#[test]
fn a_witness_of_life_vetoes() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let c = NodeKey::generate().expect("key");
    let (server, seen) = server_with_sink(&[("dev-a", &a), ("dev-b", &b), ("dev-c", &c)]);
    register(&server, &a, "dev-a", T0);
    register(&server, &b, "dev-b", T0);
    register(&server, &c, "dev-c", T0);

    // One node that can reach the subject is proof it is alive, so it is never judged while that
    // witness of life stands (§6.7).
    tell(&server, &c, "dev-c", T0 + 1_000, reachable_body("dev-b"));
    tell(&server, &a, "dev-a", T0 + 1_001, unreachable_body("dev-b"));

    assert_eq!(
        row(&server, "dev-b", T0 + 1_001).judged_at_ms,
        None,
        "a witness of life vetoed the judgement"
    );
    assert!(transitions(&seen).is_empty(), "{:?}", transitions(&seen));
}

#[test]
fn a_stale_report_is_no_longer_a_witness() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let c = NodeKey::generate().expect("key");
    let (server, seen) = server_with_sink(&[("dev-a", &a), ("dev-b", &b), ("dev-c", &c)]);
    register(&server, &a, "dev-a", T0);
    register(&server, &b, "dev-b", T0);
    register(&server, &c, "dev-c", T0);

    // A first judgement, then dev-b is heard from again, so a later judgement is a **new** one.
    tell(&server, &a, "dev-a", T0, unreachable_body("dev-b"));
    tell(&server, &b, "dev-b", T0 + 1, net::heartbeat_body());

    // At `later`, dev-a's report has aged out of the freshness window; dev-a and dev-c are kept
    // online so the only thing that can drop dev-a from the witness set is the staleness of its
    // report.
    let later = T0 + REPORT_WINDOW_MS + 1;
    tell(&server, &a, "dev-a", later, net::heartbeat_body());
    tell(&server, &c, "dev-c", later, net::heartbeat_body());
    tell(&server, &c, "dev-c", later + 1, unreachable_body("dev-b"));

    let expected = vec![
        Transition::Judged(Judgement {
            peer: "dev-b".to_string(),
            witnesses: vec!["dev-a".to_string()],
            reports: 1,
        }),
        Transition::Recovered {
            peer: "dev-b".to_string(),
            method: RecoverMethod::Heartbeat,
        },
        Transition::Judged(Judgement {
            peer: "dev-b".to_string(),
            witnesses: vec!["dev-c".to_string()],
            reports: 1,
        }),
    ];
    assert_eq!(
        transitions(&seen),
        expected,
        "the stale witness dropped out"
    );
}

#[test]
fn a_subject_the_server_never_knew_is_not_judged() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    // dev-b is a known peer, but never registers: it has no row to judge.
    let (server, seen) = server_with_sink(&[("dev-a", &a), ("dev-b", &b)]);
    register(&server, &a, "dev-a", T0);

    let (_, reply) = send(&server, &a, "dev-a", T0 + 1_000, unreachable_body("dev-b"));
    assert!(
        matches!(reply, Ok(LocalReply::UnreachableReported { .. })),
        "the report is taken: {reply:?}"
    );
    assert!(
        server
            .online_at(T0 + 1_000)
            .iter()
            .all(|row| row.node_id != "dev-b"),
        "no row was invented"
    );
    assert!(transitions(&seen).is_empty());
}

#[test]
fn the_same_report_again_is_not_a_second_judgement() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let (server, seen) = server_with_sink(&[("dev-a", &a), ("dev-b", &b)]);
    register(&server, &a, "dev-a", T0);
    register(&server, &b, "dev-b", T0);

    // §6.7's pulse: the same view is re-sent each cycle while it stands, and is not a second fact.
    tell(&server, &a, "dev-a", T0 + 1_000, unreachable_body("dev-b"));
    tell(&server, &a, "dev-a", T0 + 16_000, unreachable_body("dev-b"));

    assert_eq!(transitions(&seen).len(), 1, "{:?}", transitions(&seen));
}

#[test]
fn recovery_is_being_heard_from_and_names_what_was_heard() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let (server, seen) = server_with_sink(&[("dev-a", &a), ("dev-b", &b)]);
    register(&server, &a, "dev-a", T0);
    register(&server, &b, "dev-b", T0);

    // Judged, then heard from again by its own beat (§6.6).
    tell(&server, &a, "dev-a", T0 + 1_000, unreachable_body("dev-b"));
    let (_, reply) = send(&server, &b, "dev-b", T0 + 2_000, net::heartbeat_body());
    assert!(matches!(reply, Ok(LocalReply::Beat { .. })), "{reply:?}");
    let recovered = row(&server, "dev-b", T0 + 2_000);
    assert_eq!(recovered.judged_at_ms, None, "the judgement cleared");
    assert_eq!(recovered.state, Online::Online);

    // Judged again, then recovered by a prober's answer (§6.7's `{"reachable": …}`).
    tell(&server, &a, "dev-a", T0 + 3_000, unreachable_body("dev-b"));
    let (_, reply) = send(&server, &a, "dev-a", T0 + 4_000, reachable_body("dev-b"));
    assert!(
        matches!(reply, Ok(LocalReply::ReachableReported { .. })),
        "{reply:?}"
    );
    assert_eq!(row(&server, "dev-b", T0 + 4_000).judged_at_ms, None);

    assert_eq!(
        transitions(&seen),
        vec![
            Transition::Judged(Judgement {
                peer: "dev-b".to_string(),
                witnesses: vec!["dev-a".to_string()],
                reports: 1,
            }),
            Transition::Recovered {
                peer: "dev-b".to_string(),
                method: RecoverMethod::Heartbeat,
            },
            Transition::Judged(Judgement {
                peer: "dev-b".to_string(),
                witnesses: vec!["dev-a".to_string()],
                reports: 1,
            }),
            Transition::Recovered {
                peer: "dev-b".to_string(),
                method: RecoverMethod::Probe,
            },
        ]
    );
}

#[test]
fn the_probe_and_report_bodies_are_read_apart() {
    // §6.7's probe/alive pair, and the two reports, are four different bodies.
    assert!(is_probe(&probe_body()) && is_alive(&alive_body()));
    assert_eq!(
        report_of(&unreachable_body("dev-b")),
        Some(net::Report::Unreachable("dev-b".to_string()))
    );
    assert_eq!(
        report_of(&reachable_body("dev-b")),
        Some(net::Report::Reachable("dev-b".to_string()))
    );
    assert_eq!(
        Local::of(&unreachable_body("dev-b")),
        Local::UnreachableReport {
            node_id: "dev-b".to_string()
        }
    );
    assert_eq!(
        Local::of(&reachable_body("dev-b")),
        Local::ReachableReport {
            node_id: "dev-b".to_string()
        }
    );
    // A probe is not a local frame: it is addressed to a peer and forwarded.
    assert!(matches!(
        Local::of(&probe_body()),
        Local::Unrecognised { .. }
    ));
    let _ = Registration::in_rooms(["lab"]);
    let _ = ReplayGuard::new();
}
