//! The cross-region server, proved offline (v1.0 M4d).
//!
//! ```text
//! cargo run -p net --example relay -- --self-test
//! ```
//!
//! Everything is on loopback: a server, a destination that dials in, and a sender whose
//! direct path fails. `scripts/gate.sh` runs it beside the other example proofs.
//!
//! The two checks that are the point of §6.3 are the third and the eighth: a frame for the
//! server is never handed to anybody else, and a destination the server can *see* but that
//! has not dialled in hears nothing — because the server never dials a node. Four are
//! §6.2's other two roles: the **address query** (answered with addresses and nothing else)
//! and the **published registry** (a **source**, merged with the local files winning). Three
//! are §6.7's **liveness**: a probe is answered with `alive`, an unreachable report becomes a
//! judgement once every witness agrees, and being heard from again clears it and names the
//! method.

use net::{
    deliver, hello_body, is_alive, is_probe, now_ms, verify_at, Answer, Listener, NodeKey, Path,
    PeerEntry, PeerKeys, PeersFile, RateRule, RecoverMethod, RelayClient, RelayServer,
    RelaySession, ReplayGuard, Report, Room, RoomRules, RoomsFile, SignedMessage, Transition,
    TransportConfig,
};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn check(what: &str, ok: bool, detail: String) {
    println!("{}  {what}: {detail}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}

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

/// An address nothing is listening on, so the direct path fails and the relay carries it.
fn closed_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let address = listener.local_addr().expect("addr").to_string();
    drop(listener);
    address
}

fn start_server(key: &NodeKey, rooms: RoomsFile, peers: PeersFile) -> (RelayServer, String) {
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let server = RelayServer::new(
        "server",
        key.clone(),
        peers,
        rooms,
        config(Duration::from_secs(5)),
    )
    .expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });
    (server, addr)
}

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

fn self_test() {
    let a = NodeKey::generate().expect("key");
    let b = NodeKey::generate().expect("key");
    let server_key = NodeKey::generate().expect("key");

    let mut server_peers = PeersFile::empty();
    server_peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
    server_peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
    // The room set the deployer publishes: §6.2's management hands it to any node that asks.
    let mut server_rooms = RoomsFile::empty();
    server_rooms.rooms.push(Room {
        name: "lab".to_string(),
        members: vec!["dev-a".to_string(), "dev-b".to_string()],
        rules: RoomRules::new(RateRule {
            messages: 10,
            window_seconds: 60,
        }),
    });
    let (server, addr) = start_server(&server_key, server_rooms.clone(), server_peers);

    let server_entry = entry("server", &server_key, &addr);
    let client = RelayClient::new(
        "dev-a",
        a.clone(),
        &server_entry,
        config(Duration::from_secs(5)),
    )
    .expect("client");

    // 1. A frame for a known, dialled-in destination is handed down its session.
    let b_session = RelaySession::open(
        &addr,
        "server",
        &b,
        "dev-b",
        config(Duration::from_millis(500)),
    )
    .expect("session");
    wait_until("dev-b to be dialled in", || {
        server.sessions().is_present("dev-b")
    });
    let message = SignedMessage::sign(
        &a,
        "dev-a",
        "dev-b",
        now_ms(),
        serde_json::json!({"opaque": [1, 2, 3]}),
    )
    .expect("sign");
    let path = deliver(
        closed_addr(),
        &message,
        Some(&client),
        config(Duration::from_secs(5)),
    )
    .expect("delivered");
    let received = b_session.receive().expect("read").expect("arrived");
    check(
        "a frame for a known destination arrives byte for byte",
        matches!(path, Path::Relayed { .. })
            && received.to_line().expect("line") == message.to_line().expect("line"),
        format!("{path:?}"),
    );

    // 2. And it is an ordinary §3 message there: §6 added no layer to §3.
    let mut keys = PeerKeys::new();
    keys.insert("dev-a", [a.verifying_key().expect("public")]);
    let verified = verify_at(&received, "dev-b", &keys, &mut ReplayGuard::new(), now_ms());
    check(
        "it verifies at the destination",
        verified.is_ok(),
        verified
            .map(|v| format!("from {} with body {}", v.from, v.body))
            .unwrap_or_else(|e| e.to_string()),
    );

    // 3. A frame addressed to the server is the server's own business.
    let hello = SignedMessage::sign(&a, "dev-a", "server", now_ms(), hello_body()).expect("sign");
    client
        .send_frame(&hello.to_line().expect("line"))
        .expect("handed over");
    check(
        "a frame addressed to the server is never handed on",
        b_session.receive().expect("read").is_none(),
        "the destination heard nothing".to_string(),
    );

    // 4. A destination the server does not know is refused rather than broadcast.
    let nowhere =
        SignedMessage::sign(&a, "dev-a", "dev-z", now_ms(), serde_json::json!(null)).expect("sign");
    client
        .send_frame(&nowhere.to_line().expect("line"))
        .expect("handed over");
    check(
        "a frame for a destination the server does not know is not broadcast",
        b_session.receive().expect("read").is_none(),
        "the nodes the server does know heard nothing".to_string(),
    );

    // 5. And a destination it does know still works, on the same session.
    let known = SignedMessage::sign(&a, "dev-a", "dev-b", now_ms(), serde_json::json!({"n": 2}))
        .expect("sign");
    client
        .send_frame(&known.to_line().expect("line"))
        .expect("handed over");
    check(
        "a destination the server does know still works",
        b_session.receive().expect("read").as_ref() == Some(&known),
        "the frame for dev-b came through".to_string(),
    );

    // 6. A replay is refused at the server, so the destination sees it once.
    client
        .send_frame(&known.to_line().expect("line"))
        .expect("sent again");
    check(
        "a replayed frame does not arrive a second time",
        b_session.receive().expect("read").is_none(),
        "§3.2 refused it at the server".to_string(),
    );

    // 7. A sender the server does not know is refused.
    let (unknown_server, unknown_addr) = {
        let mut peers = PeersFile::empty();
        peers.peers.push(entry("dev-b", &b, "127.0.0.1:2"));
        start_server(&server_key, RoomsFile::empty(), peers)
    };
    let stranger_client = RelayClient::new(
        "dev-a",
        a.clone(),
        &entry("server", &server_key, &unknown_addr),
        config(Duration::from_secs(5)),
    )
    .expect("client");
    stranger_client
        .send_frame(&known.to_line().expect("line"))
        .expect("handed over");
    // Give the server's own thread time to have read both frames and refused them.
    std::thread::sleep(Duration::from_millis(300));
    check(
        "a sender the server does not know gets no session",
        !unknown_server.sessions().is_present("dev-a"),
        format!("{} session(s) live", unknown_server.sessions().len()),
    );

    // 8. §6.3: the server never dials a node. A destination the server can see, whose
    //    address really is listening, hears nothing until it dials in itself.
    let b_listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let b_addr = b_listener.local_addr().expect("addr").to_string();
    let (_, reachable_addr) = {
        let mut peers = PeersFile::empty();
        peers.peers.push(entry("dev-a", &a, "127.0.0.1:1"));
        peers.peers.push(entry("dev-b", &b, &b_addr));
        start_server(&server_key, RoomsFile::empty(), peers)
    };
    let dialer = RelayClient::new(
        "dev-a",
        a.clone(),
        &entry("server", &server_key, &reachable_addr),
        config(Duration::from_secs(5)),
    )
    .expect("client");
    dialer
        .send_frame(&known.to_line().expect("line"))
        .expect("handed over");
    b_listener.set_nonblocking(true).expect("nonblocking");
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut dialled = false;
    while Instant::now() < deadline {
        match b_listener.accept() {
            Ok(_) => {
                dialled = true;
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("the listener failed: {error}"),
        }
    }
    check(
        "the server never dials a destination that has not dialled in",
        !dialled,
        format!("{b_addr} heard nothing"),
    );

    // 9. The hello body is what opens a session, and it says the protocol version.
    check(
        "the session opener is a hello at this protocol version",
        net::is_hello(&hello_body()),
        format!("{}", hello_body()),
    );

    // 10. Signalling: where another node is, asked and answered (§6.2). The answer is the
    //     server's own signed frame, so the node verifies it as it verifies anything else.
    let mut guard = ReplayGuard::new();
    let server_keys = client.server_keys().expect("the server is a peer");
    client.query_addresses("dev-b").expect("asked");
    let answer = client.receive().expect("read").expect("an answer arrived");
    let verified = verify_at(&answer, "dev-a", &server_keys, &mut guard, now_ms())
        .expect("the server's answer verifies");
    let addresses = match Answer::from_verified(&verified, "server")
        .expect("readable")
        .expect("an answer rather than a relayed frame")
    {
        Answer::Addresses { addresses } => addresses,
        other => panic!("expected addresses, got {other:?}"),
    };
    check(
        "an address query is answered with the addresses the server knows, and nothing else",
        addresses.iter().any(|address| address == "127.0.0.1:2")
            && addresses.len() > 1
            && verified.body.as_object().map(|object| object.len()) == Some(1),
        format!("{addresses:?}"),
    );

    // 11. A node the server cannot place is answered with nothing, not refused: the question
    //     was well-formed, and "nowhere I know" is the honest answer to it.
    client.query_addresses("dev-nobody").expect("asked");
    let answer = client.receive().expect("read").expect("an answer arrived");
    let verified =
        verify_at(&answer, "dev-a", &server_keys, &mut guard, now_ms()).expect("verifies");
    let empty = match Answer::from_verified(&verified, "server")
        .expect("readable")
        .expect("an answer")
    {
        Answer::Addresses { addresses } => addresses,
        other => panic!("expected addresses, got {other:?}"),
    };
    check(
        "an address query for a node the server cannot place comes back empty",
        empty.is_empty(),
        format!("{empty:?}"),
    );

    // 12. Management: the registry, published on request — the table and the room definitions.
    client.request_registry().expect("asked");
    let answer = client.receive().expect("read").expect("an answer arrived");
    let verified =
        verify_at(&answer, "dev-a", &server_keys, &mut guard, now_ms()).expect("verifies");
    let registry = match Answer::from_verified(&verified, "server")
        .expect("readable")
        .expect("an answer")
    {
        Answer::Registry(registry) => registry,
        other => panic!("expected a registry, got {other:?}"),
    };
    check(
        "a registry request is answered with the table and the room definitions",
        registry.table().entries().len() == 2
            && registry.rooms().room("lab").is_some()
            && registry.generation() == net::FIRST_GENERATION,
        format!(
            "generation {}, {} peer(s), {} room(s)",
            registry.generation(),
            registry.table().entries().len(),
            registry.rooms().len()
        ),
    );

    // 13. And it is a **source**, not an authority: the local files win, and the disagreement
    //     is reported rather than resolved (§6.2, §4.1).
    let mut local_peers = PeersFile::empty();
    local_peers.peers.push(entry("dev-b", &b, "127.0.0.1:99"));
    let merged = registry.merge(&local_peers, &RoomsFile::empty());
    check(
        "a published registry is a source: the local file wins and the conflict is reported",
        merged.has_conflicts()
            && merged.peers.entry("dev-b").expect("dev-b").addresses
                == vec!["127.0.0.1:99".to_string()]
            && merged.rooms_report.added == 1,
        format!(
            "{} peer conflict(s), {} room(s) added",
            merged.peers_report.conflicts.len(),
            merged.rooms_report.added
        ),
    );

    // 14. Registration and heartbeat (§6.6): a node reports itself upward, and the server keeps a row.
    client
        .register(&net::Registration::in_rooms(["lab"]))
        .expect("registered");
    let ack = client
        .receive()
        .expect("read")
        .expect("an acknowledgement arrived");
    let verified = verify_at(&ack, "dev-a", &server_keys, &mut guard, now_ms()).expect("verifies");
    check(
        "a registration is answered, and the server keeps a row for the node",
        net::is_registered(&verified.body)
            && server.online().iter().any(|row| row.node_id == "dev-a"),
        format!("{} row(s)", server.online().len()),
    );

    // 15. A beat refreshes the row, and a row is **kept** when it ages out of the window: "offline" and
    //     "never registered" have to stay distinguishable (§6.6).
    let before = server.online().first().map(|row| row.last_heartbeat_ms);
    client.heartbeat().expect("beat");
    std::thread::sleep(Duration::from_millis(50));
    let after = server.online().first().map(|row| row.last_heartbeat_ms);
    let aged = server.online_at(now_ms() + net::ONLINE_WINDOW_MS + 1);
    check(
        "a beat refreshes the row, and the row survives going offline",
        after >= before && aged.len() == 1 && aged[0].state == net::Online::Offline,
        format!("{aged:?}"),
    );

    // 16. `register` and `registry` are one letter apart and opposite directions (§6.6 up, §6.2 down).
    check(
        "a registry request is not read as a registration",
        net::Local::of(&net::registry_request_body()) == net::Local::Registry
            && matches!(
                net::Local::of(&net::register_body(&[], &[], &[])),
                net::Local::Register(_)
            ),
        "register ≠ registry".to_string(),
    );

    // 17. §6.7's probe: dev-a asks dev-b, and the answer comes back — evidence about the peer's
    //     key-holder, not merely that a socket is open. A transition sink records the judgements.
    let seen: Arc<Mutex<Vec<Transition>>> = Arc::new(Mutex::new(Vec::new()));
    {
        let captured = Arc::clone(&seen);
        server.set_transition_sink(Arc::new(move |transition| {
            captured.lock().expect("seen").push(transition.clone());
        }));
    }
    // Both nodes register, so both have rows a judgement can name.
    client
        .register(&net::Registration::in_rooms(["lab"]))
        .expect("registered");
    let _ = client.receive();
    b_session
        .register(&net::Registration::in_rooms(["lab"]))
        .expect("registered");
    let _ = b_session.receive();

    client.probe("dev-b").expect("asked");
    let probe = b_session.receive().expect("read").expect("a probe arrived");
    check(
        "a probe reaches the peer, and carries §6.7's probe body",
        is_probe(&probe.body) && probe.from == "dev-a",
        format!("from {} body {}", probe.from, probe.body),
    );
    b_session.answer_alive("dev-a").expect("answered");
    let alive = client.receive().expect("read").expect("an answer arrived");
    check(
        "the peer answers with alive, addressed back",
        is_alive(&alive.body) && alive.from == "dev-b",
        format!("from {} body {}", alive.from, alive.body),
    );

    // 18. A report, then the collective judgement (§6.7): unanimity among the witnesses that
    //     remain. The server judges, and the row gains `judged_at_ms`.
    client
        .report(&Report::Unreachable("dev-b".to_string()))
        .expect("reported");
    wait_until("dev-b to be judged", || {
        server
            .online()
            .iter()
            .any(|row| row.node_id == "dev-b" && row.judged_at_ms.is_some())
    });
    let judged = seen.lock().expect("seen").clone();
    check(
        "an unreachable report is a judgement once every witness agrees",
        judged.iter().any(
            |transition| matches!(transition, Transition::Judged(judgement) if judgement.peer == "dev-b"),
        ),
        format!("{judged:?}"),
    );

    // 19. Recovery is being heard from, not a re-admission: the judged node's own beat clears it,
    //     and the transition names what was heard (§6.7).
    b_session.heartbeat().expect("beat");
    wait_until("dev-b to recover", || {
        seen.lock().expect("seen").iter().any(
            |transition| matches!(transition, Transition::Recovered { peer, .. } if peer == "dev-b"),
        )
    });
    let recovered_row = server
        .online()
        .into_iter()
        .find(|row| row.node_id == "dev-b")
        .expect("a row for dev-b");
    let transitions_now = seen.lock().expect("seen").clone();
    check(
        "being heard from again clears the judgement and names the method",
        recovered_row.judged_at_ms.is_none()
            && transitions_now.iter().any(|transition| {
                matches!(
                    transition,
                    Transition::Recovered {
                        method: RecoverMethod::Heartbeat,
                        ..
                    }
                )
            }),
        format!("{transitions_now:?}"),
    );

    println!("net relay self-test: OK");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        self_test();
        return;
    }
    println!("usage: cargo run -p net --example relay -- --self-test");
    println!("stands a cross-region server up and carries a frame through it.");
}
