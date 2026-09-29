//! The chain digest, over a session and at the table (v1.0 M4e-1).
//!
//! [connection.md §7](../../docs/connection.md) is the shape; these check the transport half of it. The
//! one that matters most is the last: reading a digest is a **read of the chain**, so a store whose
//! digest was just taken still verifies — the formula did not move (decisions §127 point 2).

use net::{
    ChainDigest, Listener, Local, LocalReply, NodeKey, PeerEntry, PeersFile, RelayClient,
    RelayError, RelayServer, RoomsFile, SignedMessage, TransportConfig,
};
use std::net::{TcpListener, TcpStream};
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

/// A server that knows `dev-a` — and **has a session for it**, so a frame can be answered.
fn server_knowing_a() -> (RelayServer, NodeKey) {
    let a = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");
    let mut peers = PeersFile::empty();
    peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    let server = RelayServer::new(
        "server",
        server_key,
        peers,
        RoomsFile::empty(),
        config(Duration::from_secs(5)),
    )
    .expect("server");

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let peer = TcpStream::connect(addr).expect("connect");
    let (session, _) = listener.accept().expect("accept");
    server.sessions().bind("dev-a", session, addr);
    std::mem::forget(peer);
    (server, a)
}

/// Sign `body` from `dev-a` at `ts`, route it, and let the server handle it.
fn send(
    server: &RelayServer,
    key: &NodeKey,
    ts: i64,
    body: serde_json::Value,
) -> (Local, Result<LocalReply, RelayError>) {
    let message = SignedMessage::sign(key, "dev-a", "server", ts, body).expect("sign");
    let frame = message.to_line().expect("line");
    let routed = server.route(&frame, ts).expect("routed");
    let action = routed.local().expect("a local frame").clone();
    let reply = server.answer_local(routed.from(), &action, ts);
    (action, reply)
}

#[test]
fn the_server_takes_a_digest_and_keeps_the_latest() {
    let (server, a) = server_knowing_a();

    let (action, reply) = send(&server, &a, T0, net::digest_body(Some("abc"), 7));
    assert_eq!(
        action,
        Local::Digest {
            chain: Some("abc".to_string()),
            length: 7
        }
    );
    assert!(
        matches!(reply, Ok(LocalReply::DigestTaken { length: 7, .. })),
        "a digest is taken, not answered: {reply:?}"
    );
    assert_eq!(
        server.digest_of("dev-a"),
        Some(ChainDigest {
            chain: Some("abc".to_string()),
            length: 7
        })
    );

    // A later report replaces the earlier one: the server holds the **latest** digest per node.
    let _ = send(&server, &a, T0 + 1, net::digest_body(Some("def"), 9));
    assert_eq!(
        server.digest_of("dev-a"),
        Some(ChainDigest {
            chain: Some("def".to_string()),
            length: 9
        })
    );
    assert_eq!(server.digests().len(), 1, "one node, one digest");
}

#[test]
fn a_digest_reaches_the_server_over_a_real_session() {
    let a = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");
    let mut peers = PeersFile::empty();
    peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));

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

    let server_entry = entry("server", &server_key, &addr);
    let client = RelayClient::new("dev-a", a, &server_entry, config(Duration::from_secs(5)))
        .expect("client");
    client
        .digest(&ChainDigest {
            chain: Some("feed".to_string()),
            length: 42,
        })
        .expect("send");

    let deadline = Instant::now() + Duration::from_secs(2);
    while server.digest_of("dev-a").is_none() {
        assert!(Instant::now() < deadline, "the digest never landed");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        server.digest_of("dev-a"),
        Some(ChainDigest {
            chain: Some("feed".to_string()),
            length: 42
        })
    );
}

#[test]
fn reading_a_digest_does_not_touch_the_chain() {
    // What decisions §127 point 2 protects: the digest is a **read** of the head and the length, and the
    // chain verifies exactly as it did — the formula, the `prev_hash` linkage and every row's hash are
    // untouched.
    let mut store = audit::AuditStore::in_memory().expect("store");
    store
        .append(audit::AuditEvent::new(
            "sandbox",
            "vm.start",
            serde_json::json!({ "n": 1 }),
        ))
        .expect("append");
    store
        .append(audit::AuditEvent::new(
            "sandbox",
            "vm.stop",
            serde_json::json!({ "n": 2 }),
        ))
        .expect("append");

    let before = audit::verify_chain(&store).expect("verify");
    assert!(matches!(before, audit::ChainStatus::Intact { length: 2 }));

    let digest = ChainDigest::of(&store).expect("digest");
    assert_eq!(digest.length, 2, "the digest counts the chain's events");
    assert_eq!(
        digest.chain,
        store.last_hash().expect("head"),
        "the digest's head is the chain's last hash"
    );

    assert_eq!(
        audit::verify_chain(&store).expect("verify"),
        before,
        "taking a digest changed nothing about the chain"
    );
}
