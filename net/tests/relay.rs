//! The cross-region server, end to end over loopback (v1.0 M4d).
//!
//! [connection.md §6](../../docs/connection.md) is what these check, and the ones worth
//! naming are the two that sound like details and are not: a frame **addressed to the
//! server itself is never handed on**, and the server **never dials a node** — a
//! destination with a perfectly reachable address hears nothing until it dials in and
//! holds the session open. The rest is §3.1's relay leg doing what it says: the bytes that
//! arrive are the bytes the sender signed, they verify at the destination as an ordinary
//! message, and a replay is refused at the server.

use net::{
    deliver, hello_body, is_key_event, now_ms, verify_at, KeyEvent, Listener, NodeKey, Path,
    PeerEntry, PeerKeys, PeersFile, RelayClient, RelayServer, RelaySession, ReplayGuard, RoomsFile,
    SignedMessage, TransportConfig,
};
use std::net::TcpListener;
use std::time::{Duration, Instant};

/// A transport configuration with a read timeout the test can choose: the "nothing
/// arrived" checks must be able to end, and the "something arrived" ones must not.
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

/// An address nothing is listening on: bound, then released, so a direct attempt at it
/// fails and the relay leg is the one that carries the frame.
fn closed_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr").to_string();
    drop(listener);
    address
}

/// A server, listening, on its own thread.
struct Running {
    server: RelayServer,
    addr: String,
}

fn start_server(node_id: &str, peers: PeersFile, read: Duration) -> Running {
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    // These tests are about the relay half, so the key and the room set are the minimum a
    // server needs to start; `server.rs` is where §6.2's other two roles are exercised.
    let server = RelayServer::new(
        node_id,
        NodeKey::generate().expect("key"),
        peers,
        RoomsFile::empty(),
        config(read),
    )
    .expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });
    Running { server, addr }
}

/// Wait for a condition the server's own thread reaches, or give up after a second.
fn wait_until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

#[test]
fn a_frame_reaches_the_destination_through_the_server() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    let running = start_server("server", server_peers, Duration::from_secs(5));

    // B dials out and keeps the session open. That is what makes it reachable: the
    // server will hand frames down this socket and never open one of its own.
    let b_session = RelaySession::open(
        &running.addr,
        "server",
        &b,
        "dev-b",
        config(Duration::from_secs(5)),
    )
    .expect("session");
    wait_until("dev-b to be dialled in", || {
        running.server.sessions().is_present("dev-b")
    });

    // A cannot reach B directly, so §3.1 hands the frame to the relay.
    let server_entry = entry("server", &server_key, &running.addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");
    let message = SignedMessage::sign(
        &a,
        "dev-a",
        "dev-b",
        now_ms(),
        serde_json::json!({"hello": "world"}),
    )
    .expect("sign");
    let path = deliver(
        closed_addr(),
        &message,
        Some(&client),
        config(Duration::from_secs(5)),
    )
    .expect("delivered");
    assert!(matches!(path, Path::Relayed { .. }), "got {path:?}");

    let received = b_session.receive().expect("read").expect("a frame arrived");
    assert_eq!(
        received.to_line().expect("line"),
        message.to_line().expect("line"),
        "the relay changed the bytes the sender signed"
    );

    // And it verifies at B as an ordinary §3 message: §6 added no layer to §3.
    let mut keys = PeerKeys::new();
    keys.insert("dev-a", [a.verifying_key().expect("public")]);
    let verified = verify_at(&received, "dev-b", &keys, &mut ReplayGuard::new(), now_ms())
        .expect("verifies at the destination");
    assert_eq!(verified.from, "dev-a");
    assert_eq!(verified.body, serde_json::json!({"hello": "world"}));
}

#[test]
fn a_frame_addressed_to_the_server_itself_is_never_handed_on() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    let running = start_server("server", server_peers, Duration::from_secs(5));

    let b_session = RelaySession::open(
        &running.addr,
        "server",
        &b,
        "dev-b",
        config(Duration::from_millis(400)),
    )
    .expect("session");
    wait_until("dev-b to be dialled in", || {
        running.server.sessions().is_present("dev-b")
    });

    let server_entry = entry("server", &server_key, &running.addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");
    // The hello is the session opener: addressed to the server, and the server's own
    // business rather than anybody's to be handed.
    let hello = SignedMessage::sign(&a, "dev-a", "server", now_ms(), hello_body()).expect("sign");
    client
        .send_frame(&hello.to_line().expect("line"))
        .expect("handed over");

    assert!(
        b_session.receive().expect("read").is_none(),
        "a frame for the server was handed to another node"
    );
}

#[test]
fn a_sender_the_server_does_not_know_is_refused() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    // The server knows B and not A. §6.3: a node it does not know is refused.
    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    let running = start_server("server", server_peers, Duration::from_secs(5));

    let b_session = RelaySession::open(
        &running.addr,
        "server",
        &b,
        "dev-b",
        config(Duration::from_millis(400)),
    )
    .expect("session");
    wait_until("dev-b to be dialled in", || {
        running.server.sessions().is_present("dev-b")
    });

    let server_entry = entry("server", &server_key, &running.addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");
    let message =
        SignedMessage::sign(&a, "dev-a", "dev-b", now_ms(), serde_json::json!(null)).expect("sign");
    let path = deliver(
        closed_addr(),
        &message,
        Some(&client),
        config(Duration::from_secs(5)),
    )
    .expect("the write itself succeeds; the server is what refuses");
    assert!(matches!(path, Path::Relayed { .. }));

    assert!(
        b_session.receive().expect("read").is_none(),
        "an unknown sender's frame was carried anyway"
    );
    assert!(
        !running.server.sessions().is_present("dev-a"),
        "an unknown sender got a session"
    );
}

#[test]
fn a_destination_the_server_does_not_know_is_refused_rather_than_broadcast() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    let running = start_server("server", server_peers, Duration::from_secs(5));

    let b_session = RelaySession::open(
        &running.addr,
        "server",
        &b,
        "dev-b",
        config(Duration::from_millis(400)),
    )
    .expect("session");
    wait_until("dev-b to be dialled in", || {
        running.server.sessions().is_present("dev-b")
    });

    let server_entry = entry("server", &server_key, &running.addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");

    let nowhere =
        SignedMessage::sign(&a, "dev-a", "dev-z", now_ms(), serde_json::json!(null)).expect("sign");
    client
        .send_frame(&nowhere.to_line().expect("line"))
        .expect("handed over");
    assert!(
        b_session.receive().expect("read").is_none(),
        "a frame for an unknown node was sprayed at the nodes the server does know"
    );

    // And a destination it does know still works, on the same session.
    let known = SignedMessage::sign(&a, "dev-a", "dev-b", now_ms(), serde_json::json!({"n": 2}))
        .expect("sign");
    client
        .send_frame(&known.to_line().expect("line"))
        .expect("handed over");
    let received = b_session.receive().expect("read").expect("a frame arrived");
    assert_eq!(received, known);
}

#[test]
fn a_replayed_frame_arrives_once() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    let running = start_server("server", server_peers, Duration::from_secs(5));

    let b_session = RelaySession::open(
        &running.addr,
        "server",
        &b,
        "dev-b",
        config(Duration::from_millis(400)),
    )
    .expect("session");
    wait_until("dev-b to be dialled in", || {
        running.server.sessions().is_present("dev-b")
    });

    let server_entry = entry("server", &server_key, &running.addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");
    let message = SignedMessage::sign(&a, "dev-a", "dev-b", now_ms(), serde_json::json!({"n": 1}))
        .expect("sign");
    let frame = message.to_line().expect("line");

    client.send_frame(&frame).expect("first");
    client
        .send_frame(&frame)
        .expect("the second write succeeds; §3.2 is what refuses it");

    assert_eq!(
        b_session.receive().expect("read").expect("arrived"),
        message
    );
    assert!(
        b_session.receive().expect("read").is_none(),
        "the replayed frame was carried a second time"
    );
}

#[test]
fn a_direct_connection_leaves_the_relay_idle() {
    let a = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    // A destination that really can be dialled, listening on its own port.
    let b_listener = Listener::bind("127.0.0.1:0").expect("bind");
    let direct = b_listener.local_addr().expect("addr").to_string();

    let running = start_server("server", PeersFile::empty(), Duration::from_secs(5));
    let server_entry = entry("server", &server_key, &running.addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");

    let message = SignedMessage::sign(&a, "dev-a", "dev-b", now_ms(), serde_json::json!({"d": 1}))
        .expect("sign");
    let path = deliver(
        &direct,
        &message,
        Some(&client),
        config(Duration::from_secs(5)),
    )
    .expect("delivered");
    assert_eq!(path, Path::Direct);
    assert!(
        !client.is_connected(),
        "a direct connection should not have dialled the relay"
    );

    let mut connection = b_listener.accept().expect("accept");
    assert_eq!(connection.receive().expect("read"), message);
}

#[test]
fn the_server_never_dials_a_destination_that_has_not_dialled_in() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    // B is reachable — it has a live listener on a real address — and B is in the
    // server's table with that address. It has simply not dialled in.
    let b_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let b_addr = b_listener.local_addr().expect("addr").to_string();

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    server_peers.peers.push(entry("dev-b", &b, &b_addr));
    let running = start_server("server", server_peers, Duration::from_secs(5));

    let server_entry = entry("server", &server_key, &running.addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");
    let message =
        SignedMessage::sign(&a, "dev-a", "dev-b", now_ms(), serde_json::json!(null)).expect("sign");
    client
        .send_frame(&message.to_line().expect("line"))
        .expect("handed over");

    // §6.3: the server hands a frame down a session, and there is no session. Its
    // knowledge of B's address is for signalling, not for dialling.
    b_listener.set_nonblocking(true).expect("nonblocking");
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        match b_listener.accept() {
            Ok(_) => panic!("the server dialled a node"),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("the listener failed: {error}"),
        }
    }
    // The frame was refused, not stored: the sender's own session is live, and the
    // destination the server could see but that never dialled in is not.
    assert!(!running.server.sessions().is_present("dev-b"));
    assert!(running.server.sessions().is_present("dev-a"));
}

#[test]
fn a_key_event_is_pushed_at_once_and_kept_in_a_bounded_log() {
    // v1.0 M4e-2: a key event is a **fact**, so the server keeps a log of them rather than a latest
    // value (which is what a digest is), the log is bounded, and a re-pushed fact is the same fact.
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

    let fork = KeyEvent {
        at_ms: 1_700_000_000_000,
        action: "host.audit.segment_forked".to_string(),
        detail: serde_json::json!({ "segment_id": "seg-dev-a-1" }),
    };
    client.key_event(&fork).expect("pushed");
    client.key_event(&fork).expect("the same fact pushed again");
    wait_until("the key event to land", || {
        !server.key_events_of("dev-a").is_empty()
    });
    assert_eq!(
        server.key_events_of("dev-a").len(),
        1,
        "a re-pushed fact is not a second fact"
    );

    // Past the bound the oldest fall off, and what stays is in the order it arrived.
    let extra = net::KEY_EVENT_LOG + 4;
    for n in 0..extra {
        let filler = KeyEvent {
            at_ms: 1_000 + n as i64,
            action: "host.test.key".to_string(),
            detail: serde_json::json!({ "n": n }),
        };
        client.key_event(&filler).expect("pushed");
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let newest = extra - 1;
    while Instant::now() < deadline
        && !server
            .key_events_of("dev-a")
            .iter()
            .any(|held| held.detail == serde_json::json!({ "n": newest }))
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let log = server.key_events_of("dev-a");
    assert_eq!(
        log.len(),
        net::KEY_EVENT_LOG,
        "the log is bounded: got {}",
        log.len()
    );
    assert_eq!(
        log.last().expect("newest").detail,
        serde_json::json!({ "n": newest }),
        "the newest event is last"
    );
    assert!(
        !log.iter()
            .any(|held| held.action == "host.audit.segment_forked"),
        "the oldest fell off the bounded log"
    );

    // The body is an ordinary §3 body, and it is not any other body.
    assert_eq!(KeyEvent::of_body(&fork.to_body()), Some(fork.clone()));
    assert!(!is_key_event(&net::digest_body(None, 0)));
    assert_eq!(net::ChainDigest::of_body(&fork.to_body()), None);
    assert!(matches!(
        net::Local::of(&fork.to_body()),
        net::Local::KeyEvent { .. }
    ));
}
