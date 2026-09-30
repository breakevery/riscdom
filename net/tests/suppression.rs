//! The takeover broadcast, over a real relay (v1.0 M5-3b-2).
//!
//! [decisions §33](../../docs/decisions.md)'s third layer reacts to a broadcast that travels **between
//! peers**. In this deployment the peers share the cross-region server's relay, so the broadcast leaves
//! through the sender's session and arrives down the receiver's — the same path §6.7's reports take. This
//! checks that whole path once, which is the gap batch BW's unit tests left.

use net::{
    is_segment_done, is_segment_event, is_takeover, Listener, NodeKey, PeerEntry, PeersFile,
    RelayClient, RelayServer, RelaySession, RoomsFile, SegmentDone, SegmentEvent, TransportConfig,
};
use std::time::{Duration, Instant};

const T0: i64 = 1_700_000_000_000;

fn config(read: Duration) -> TransportConfig {
    TransportConfig {
        connect_timeout: Duration::from_secs(2),
        read_timeout: read,
        write_timeout: Duration::from_secs(2),
        ..TransportConfig::default()
    }
}

fn entry(node_id: &str, key: &NodeKey, address: &str) -> PeerEntry {
    PeerEntry::new(node_id, address, key.public_jwk())
}

#[test]
fn a_takeover_broadcast_reaches_a_peer_through_the_relay() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");
    let mut peers = PeersFile::empty();
    peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));

    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let server = RelayServer::new(
        "server",
        server_key.clone(),
        peers,
        RoomsFile::empty(),
        config(Duration::from_secs(5)),
    )
    .expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });

    // B dials out and keeps the session open — that is what makes it reachable (§6.3).
    let b_session =
        RelaySession::open(&addr, "server", &b, "dev-b", config(Duration::from_secs(5)))
            .expect("session");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !server.sessions().is_present("dev-b") {
        assert!(Instant::now() < deadline, "dev-b never dialled in");
        std::thread::sleep(Duration::from_millis(10));
    }

    // A announces the takeover, addressed to its peer.
    let server_entry = entry("server", &server_key, &addr);
    let client = RelayClient::new("dev-a", a, &server_entry, config(Duration::from_secs(5)))
        .expect("client");
    client
        .takeover_to("dev-b", "centre", "dev-a", T0)
        .expect("send");

    let received = b_session.receive().expect("read").expect("a frame arrived");
    assert_eq!(received.from, "dev-a");
    assert_eq!(
        received.to, "dev-b",
        "the broadcast is addressed to the peer"
    );
    let takeover = is_takeover(&received.body).expect("a takeover body");
    assert_eq!(takeover.centre, "centre");
    assert_eq!(takeover.by, "dev-a");
    assert_eq!(takeover.at_ms, T0);
}

#[test]
fn a_segment_stream_reaches_the_centre_through_the_relay() {
    // v1.0 M5-3c-2: a closed segment travels to the centre as frames — one per event, then the end.
    // The same path the reports and the broadcast take; nothing new on the server.
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");
    let mut peers = PeersFile::empty();
    peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));

    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let server = RelayServer::new(
        "server",
        server_key.clone(),
        peers,
        RoomsFile::empty(),
        config(Duration::from_secs(5)),
    )
    .expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });

    let b_session =
        RelaySession::open(&addr, "server", &b, "dev-b", config(Duration::from_secs(5)))
            .expect("session");
    let deadline = Instant::now() + Duration::from_secs(2);
    while !server.sessions().is_present("dev-b") {
        assert!(Instant::now() < deadline, "dev-b never dialled in");
        std::thread::sleep(Duration::from_millis(10));
    }

    let server_entry = entry("server", &server_key, &addr);
    let client = RelayClient::new("dev-a", a, &server_entry, config(Duration::from_secs(5)))
        .expect("client");
    let event = SegmentEvent {
        segment_id: "seg-dev-a-1".to_string(),
        centre: "centre".to_string(),
        index: 0,
        total: 1,
        ts: T0,
        actor: "host".to_string(),
        action: "host.test.range".to_string(),
        agent_id: Some("dev-a-1-1".to_string()),
        detail: serde_json::json!({ "n": 1 }),
    };
    client
        .segment_event_to("dev-b", &event)
        .expect("send event");
    let done = SegmentDone {
        segment_id: "seg-dev-a-1".to_string(),
        centre: "centre".to_string(),
        total: 1,
        head_prev_chain: Some("deadbeef".to_string()),
    };
    client.segment_done_to("dev-b", &done).expect("send done");

    let received = b_session.receive().expect("read").expect("a frame");
    assert_eq!(is_segment_event(&received.body).as_ref(), Some(&event));
    assert!(
        is_segment_done(&received.body).is_none(),
        "an event frame is not an end frame"
    );
    let received = b_session.receive().expect("read").expect("a frame");
    assert_eq!(is_segment_done(&received.body).as_ref(), Some(&done));
    assert!(
        is_segment_event(&received.body).is_none(),
        "an end frame is not an event frame"
    );
    // And neither is any other body this node knows.
    assert!(is_takeover(&received.body).is_none());
}
