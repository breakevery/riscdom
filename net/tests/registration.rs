//! Registration and heartbeat, at the table and over a session (v1.0 V-2).
//!
//! [connection.md §6.6](../../docs/connection.md) freezes the shapes; these check the server's half of
//! them. The two that matter most are the ones a careless reading gets wrong: a beat **places nobody**
//! (a row is created by a registration), and a row that goes **offline** is **kept** — because
//! "offline" and "never registered" have to stay distinguishable.

use net::{
    now_ms, register_body, verify_at, Listener, Local, LocalReply, NodeKey, Online, PeerEntry,
    PeersFile, Registration, RelayClient, RelayError, RelayServer, ReplayGuard, RoomsFile,
    SignedMessage, TransportConfig, ONLINE_WINDOW_MS,
};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

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

/// A server that knows `dev-a` — and **has a session for it**, so the frames can be answered.
///
/// `answer_local` writes an answer down the sender's session, so a fixture without one could take a
/// registration into the table and still fail to answer it. The session here is a loopback socket pair,
/// with the far end deliberately left open (`forget`) so a write into it succeeds.
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
fn a_registration_creates_the_row_and_a_beat_places_nobody() {
    let (server, a) = server_knowing_a();

    // A beat first: §6.6 creates a row by a **registration**, so this places nobody.
    let (action, reply) = send(&server, &a, T0, net::heartbeat_body());
    assert_eq!(action, Local::Heartbeat, "the body is read as a heartbeat");
    assert!(
        matches!(reply, Ok(LocalReply::Unplaced { .. })),
        "a beat with no row places nobody: {reply:?}"
    );
    assert!(server.online().is_empty(), "a beat alone is not a row");

    // The registration creates it, with the claims it carried.
    let (action, reply) = send(
        &server,
        &a,
        T0 + 1,
        register_body(&[], &["build".to_string()], &["lab".to_string()]),
    );
    assert!(matches!(action, Local::Register(_)));
    assert!(
        matches!(reply, Ok(LocalReply::Registered { fresh: true, .. })),
        "the first registration creates the row: {reply:?}"
    );
    let row = server
        .online_at(T0 + 1)
        .into_iter()
        .find(|row| row.node_id == "dev-a")
        .expect("a row for dev-a");
    assert_eq!(row.rooms, vec!["lab".to_string()]);
    assert_eq!(row.capabilities, vec!["build".to_string()]);
    assert!(row.addresses.is_empty(), "a node may report no addresses");
    assert_eq!(row.state, Online::Online);

    // A second registration refreshes the row and does not make a second one.
    let (_, reply) = send(
        &server,
        &a,
        T0 + 2,
        register_body(&[], &[], &["lab".to_string(), "quiet".to_string()]),
    );
    assert!(
        matches!(reply, Ok(LocalReply::Registered { fresh: false, .. })),
        "a second registration is idempotent: {reply:?}"
    );
    assert_eq!(server.online_at(T0 + 2).len(), 1);
    assert_eq!(
        server.online_at(T0 + 2)[0].rooms,
        vec!["lab".to_string(), "quiet".to_string()],
        "the claims are replaced, not merged"
    );
}

#[test]
fn a_beat_refreshes_the_row_and_the_row_is_kept_when_it_goes_offline() {
    let (server, a) = server_knowing_a();
    let (_, reply) = send(&server, &a, T0, register_body(&[], &[], &[]));
    assert!(reply.is_ok(), "the registration was taken: {reply:?}");

    // A beat inside the window refreshes it (§6.6: three intervals is 45 s).
    let (action, reply) = send(&server, &a, T0 + 30_000, net::heartbeat_body());
    assert_eq!(action, Local::Heartbeat);
    assert!(matches!(reply, Ok(LocalReply::Beat { .. })), "{reply:?}");
    assert_eq!(
        server.online_at(T0 + 30_000)[0].last_heartbeat_ms,
        T0 + 30_000
    );
    assert_eq!(server.online_at(T0 + 30_000)[0].state, Online::Online);

    // The boundary itself is inside; a millisecond past it is not.
    let boundary = T0 + 30_000 + ONLINE_WINDOW_MS;
    assert_eq!(server.online_at(boundary)[0].state, Online::Online);
    assert_eq!(server.online_at(boundary + 1)[0].state, Online::Offline);

    // And the row is **kept**: offline is not the same statement as "never registered".
    assert_eq!(server.online_at(boundary + 1).len(), 1);
    assert_eq!(server.online_at(boundary + 1)[0].node_id, "dev-a");
}

#[test]
fn register_and_registry_are_two_different_frames() {
    // One letter apart, opposite directions: a registry request must never be read as a registration.
    let (server, a) = server_knowing_a();
    let (action, reply) = send(&server, &a, T0, net::registry_request_body());
    assert_eq!(action, Local::Registry);
    assert!(
        matches!(reply, Ok(LocalReply::Registry { .. })),
        "{reply:?}"
    );
    assert!(server.online().is_empty(), "asking is not registering");
}

#[test]
fn a_client_registers_beats_and_is_answered_once() {
    // The wire half: a real session, a registration that is acknowledged, and a beat that is not.
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
        config(Duration::from_millis(400)),
    )
    .expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });

    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &entry("server", &server_key, &addr),
        config(Duration::from_millis(400)),
    )
    .expect("client");
    // Built without dialling (v1.0 V-2's laziness); `connect` is what dials.
    assert!(!client.is_connected(), "a client is inert until it is used");
    client.connect().expect("connected");
    assert!(client.is_connected());

    client
        .register(&Registration::in_rooms(["lab"]))
        .expect("registered");
    // The acknowledgement is the server's own signed frame.
    let ack = client
        .receive()
        .expect("read")
        .expect("a registration is answered");
    let mut keys = net::PeerKeys::new();
    keys.insert("server", [server_key.verifying_key().expect("public")]);
    let verified = verify_at(&ack, "dev-a", &keys, &mut ReplayGuard::new(), now_ms())
        .expect("the acknowledgement verifies");
    assert!(net::is_registered(&verified.body), "{:?}", verified.body);

    // The row is there, and a beat refreshes it — and is answered with **nothing**.
    assert_eq!(server.online().len(), 1);
    let first = server.online()[0].last_heartbeat_ms;
    client.heartbeat().expect("beat");
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        client.receive().expect("read").is_none(),
        "a heartbeat is a statement, not a question"
    );
    assert!(
        server.online()[0].last_heartbeat_ms >= first,
        "the beat advanced the row"
    );
    assert_eq!(server.online()[0].state, Online::Online);
    assert_eq!(server.online()[0].rooms, vec!["lab".to_string()]);
}

#[test]
fn a_frame_the_server_does_not_know_is_never_a_registration() {
    // §6.3's authorisation is what keeps a key from arriving by frame: a stranger's registration is
    // refused before any of this is reached, so no row appears for it.
    let (server, _a) = server_knowing_a();
    let stranger = NodeKey::generate().expect("key");
    let message = SignedMessage::sign(
        &stranger,
        "dev-stranger",
        "server",
        T0,
        register_body(&[], &[], &[]),
    )
    .expect("sign");
    let frame = message.to_line().expect("line");
    assert!(matches!(
        server.route(&frame, T0),
        Err(RelayError::Refused(_))
    ));
    assert!(server.online().is_empty());
}
