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
//! has not dialled in hears nothing — because the server never dials a node.

use net::{
    deliver, hello_body, now_ms, verify_at, Listener, NodeKey, Path, PeerEntry, PeerKeys,
    PeersFile, RelayClient, RelayServer, RelaySession, ReplayGuard, SignedMessage, TransportConfig,
};
use std::net::TcpListener;
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

fn start_server(peers: PeersFile) -> (RelayServer, String) {
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let server = RelayServer::new("server", peers, config(Duration::from_secs(5))).expect("server");
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
    let (server, addr) = start_server(server_peers);

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
        start_server(peers)
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
        start_server(peers)
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
