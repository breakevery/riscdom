//! The cross-region server's local roles, end to end over loopback (v1.0 M4d).
//!
//! [connection.md §6.2](../../docs/connection.md) gives the server four roles; the relay's is
//! in `relay.rs`. These check the other two. **Signalling** answers where a `node_id` can be
//! reached, and it knows **addresses, never payloads** — the answer body has one member and a
//! node the server cannot place comes back empty rather than refused. **Management** publishes
//! the node table and the room definitions, and it is a **source and not an authority**: the
//! local `peers.json` and `rooms.json` win, and every disagreement is reported.
//!
//! The authorisation is §3's model in both cases — a known sender with a verifying signature,
//! no new credential — which is why the last test is about a stranger getting no answer.

use net::{
    now_ms, verify_at, Answer, Listener, NodeKey, PeerEntry, PeersFile, RateRule, RelayClient,
    RelayServer, RelaySession, ReplayGuard, Room, RoomRules, RoomsFile, TransportConfig,
    VerifiedMessage,
};
use std::time::Duration;

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

fn room(name: &str, members: &[&str]) -> Room {
    Room {
        name: name.to_string(),
        members: members.iter().map(|member| member.to_string()).collect(),
        rules: RoomRules::new(RateRule {
            messages: 10,
            window_seconds: 60,
        }),
    }
}

fn rooms(definition: Vec<Room>) -> RoomsFile {
    RoomsFile {
        schema_version: RoomsFile::SCHEMA_VERSION,
        rooms: definition,
    }
}

/// A server, listening, on its own thread.
struct Running {
    server: RelayServer,
    addr: String,
}

fn start_server(
    node_id: &str,
    key: &NodeKey,
    peers: PeersFile,
    rooms: RoomsFile,
    read: Duration,
) -> Running {
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let server =
        RelayServer::new(node_id, key.clone(), peers, rooms, config(read)).expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });
    Running { server, addr }
}

/// Send one question from `dev-a` and read the server's answer, verified as any §3 message is.
fn ask(
    client: &RelayClient,
    guard: &mut ReplayGuard,
    question: impl FnOnce(&RelayClient),
) -> VerifiedMessage {
    question(client);
    let message = client
        .receive()
        .expect("read")
        .expect("the server answered");
    verify_at(
        &message,
        "dev-a",
        &client.server_keys().expect("the server is a peer"),
        guard,
        now_ms(),
    )
    .expect("the server's answer verifies")
}

/// The addresses of an answer, or a panic that says what came instead.
fn addresses(verified: &VerifiedMessage) -> Vec<String> {
    match Answer::from_verified(verified, "server")
        .expect("readable")
        .expect("an answer rather than a relayed frame")
    {
        Answer::Addresses { addresses } => addresses,
        other => panic!("expected addresses, got {other:?}"),
    }
}

/// The registry of an answer.
fn registry(verified: &VerifiedMessage) -> net::Registry {
    match Answer::from_verified(verified, "server")
        .expect("readable")
        .expect("an answer")
    {
        Answer::Registry(registry) => registry,
        other => panic!("expected a registry, got {other:?}"),
    }
}

/// Two nodes, a server that knows both, and a node that dialled in. The common setup.
struct Fixture {
    running: Running,
    a: NodeKey,
    b: NodeKey,
    client: RelayClient,
    /// Held so B stays dialled in: drop it and the socket closes, and signalling would have
    /// nothing left to report.
    _b_session: RelaySession,
}

fn fixture(server_rooms: RoomsFile) -> Fixture {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    let running = start_server(
        "server",
        &server_key,
        server_peers,
        server_rooms,
        Duration::from_secs(5),
    );

    // B dials in, which is what makes it something signalling can report.
    let b_session = RelaySession::open(
        &running.addr,
        "server",
        &b,
        "dev-b",
        config(Duration::from_millis(500)),
    )
    .expect("session");

    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &entry("server", &server_key, &running.addr),
        config(Duration::from_secs(5)),
    )
    .expect("client");

    Fixture {
        running,
        a,
        b,
        client,
        _b_session: b_session,
    }
}

#[test]
fn an_address_query_is_answered_from_what_the_server_knows() {
    let fixture = fixture(RoomsFile::empty());
    // Wait until dev-b is dialled in, or the session's address would be missing.
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !fixture.running.server.sessions().is_present("dev-b") {
        assert!(
            std::time::Instant::now() < deadline,
            "dev-b never dialled in"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut guard = ReplayGuard::new();
    let verified = ask(&fixture.client, &mut guard, |client| {
        client.query_addresses("dev-b").expect("asked");
    });
    let known = addresses(&verified);

    // Both sources, and both are transport facts: the address dev-b dialled in from, and the
    // address its `peers.json` entry carries.
    assert!(
        known.contains(&"127.0.0.1:2".to_string()),
        "the entry's address is in the answer: {known:?}"
    );
    assert_eq!(
        known.len(),
        2,
        "the live session's address is in the answer too: {known:?}"
    );
}

#[test]
fn an_address_answer_carries_addresses_and_nothing_else() {
    let fixture = fixture(RoomsFile::empty());
    let mut guard = ReplayGuard::new();
    let verified = ask(&fixture.client, &mut guard, |client| {
        client.query_addresses("dev-b").expect("asked");
    });

    // §6.2: signalling "knows addresses, never payloads" — one member, nothing riding along.
    let body = verified.body.as_object().expect("a JSON object");
    assert_eq!(body.len(), 1, "the answer is exactly `addresses`: {body:?}");
    assert!(body.get("addresses").is_some());
}

#[test]
fn a_node_the_server_cannot_place_comes_back_empty() {
    let fixture = fixture(RoomsFile::empty());
    let mut guard = ReplayGuard::new();
    let verified = ask(&fixture.client, &mut guard, |client| {
        client.query_addresses("dev-nobody").expect("asked");
    });
    assert!(
        addresses(&verified).is_empty(),
        "a well-formed question gets the honest answer rather than a refusal"
    );
}

#[test]
fn a_registry_request_is_answered_with_the_table_and_the_rooms() {
    let fixture = fixture(rooms(vec![room("lab", &["dev-a", "dev-b"])]));
    let mut guard = ReplayGuard::new();
    let verified = ask(&fixture.client, &mut guard, |client| {
        client.request_registry().expect("asked");
    });
    let published = registry(&verified);

    assert_eq!(published.generation(), net::FIRST_GENERATION);
    assert_eq!(published.table().entries().len(), 2);
    assert!(published.table().entry("dev-b").is_some());
    assert_eq!(
        published.rooms(),
        &rooms(vec![room("lab", &["dev-a", "dev-b"])])
    );
}

#[test]
fn a_published_registry_is_a_source_and_the_local_files_win() {
    let fixture = fixture(rooms(vec![
        room("lab", &["dev-a"]),
        room("quiet", &["dev-b"]),
    ]));
    let mut guard = ReplayGuard::new();
    let verified = ask(&fixture.client, &mut guard, |client| {
        client.request_registry().expect("asked");
    });
    let published = registry(&verified);

    // The local side disagrees about dev-b's address and about `lab`, and has no `quiet`.
    let mut local_peers = PeersFile::empty();
    local_peers
        .peers
        .push(entry("dev-b", &fixture.b, "127.0.0.1:99"));
    let local_rooms = rooms(vec![room("lab", &["dev-b"])]);

    let merged = published.merge(&local_peers, &local_rooms);
    assert!(merged.has_conflicts());
    assert_eq!(merged.peers_report.conflicts.len(), 1);
    assert_eq!(merged.rooms_report.conflicts.len(), 1);
    assert_eq!(merged.peers_report.added, 1, "dev-a came from the table");
    assert_eq!(merged.rooms_report.added, 1, "quiet came from the registry");

    // Local wins on both halves, and nothing was resolved in silence.
    assert_eq!(
        merged.peers.entry("dev-b").expect("dev-b").addresses,
        vec!["127.0.0.1:99".to_string()]
    );
    assert_eq!(
        merged.rooms.room("lab").expect("lab").members,
        vec!["dev-b".to_string()]
    );
    assert!(merged.rooms.room("quiet").is_some());
    assert!(merged.peers.entry("dev-a").is_some());
}

#[test]
fn the_generation_advances_when_the_registry_changes() {
    let fixture = fixture(rooms(vec![room("lab", &["dev-a"])]));
    let mut guard = ReplayGuard::new();

    let first = ask(&fixture.client, &mut guard, |client| {
        client.request_registry().expect("asked");
    });
    assert_eq!(registry(&first).generation(), net::FIRST_GENERATION);

    // The deployer republishes a room set: the registry a node is handed has changed.
    fixture
        .running
        .server
        .set_rooms(rooms(vec![
            room("lab", &["dev-a"]),
            room("quiet", &["dev-b"]),
        ]))
        .expect("the rooms are usable");

    let second = ask(&fixture.client, &mut guard, |client| {
        client.request_registry().expect("asked");
    });
    let published = registry(&second);
    assert_eq!(published.generation(), net::FIRST_GENERATION + 1);
    assert_eq!(published.rooms().len(), 2);
}

#[test]
fn the_server_does_not_answer_a_sender_it_does_not_know() {
    // §6.3's authorisation is §3's model: a node the server does not know gets nothing, and
    // there is no credential it could have presented instead.
    let b = NodeKey::generate().expect("key");
    let stranger = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    let running = start_server(
        "server",
        &server_key,
        server_peers,
        RoomsFile::empty(),
        Duration::from_secs(5),
    );

    let client = RelayClient::new(
        "dev-stranger",
        stranger,
        &entry("server", &server_key, &running.addr),
        config(Duration::from_millis(400)),
    )
    .expect("client");
    // The write succeeds — the server is what refuses.
    client.query_addresses("dev-b").expect("handed over");
    assert!(
        client.receive().expect("read").is_none(),
        "an unknown sender was answered"
    );
    assert!(!running.server.sessions().is_present("dev-stranger"));
}

#[test]
fn a_local_frame_the_server_does_not_recognise_is_ignored() {
    let fixture = fixture(RoomsFile::empty());
    // A signed frame for the server that asks nothing it knows how to do.
    let message = net::SignedMessage::sign(
        &fixture.a,
        "dev-a",
        "server",
        now_ms(),
        serde_json::json!({ "what": "ever" }),
    )
    .expect("sign");
    fixture
        .client
        .send_frame(&message.to_line().expect("line"))
        .expect("handed over");
    assert!(
        fixture.client.receive().expect("read").is_none(),
        "nothing was invented for an unrecognised local frame"
    );
}
