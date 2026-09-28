//! The transport: one JSON line per message over TCP, direct first and relay second
//! (v1.0 M4a; [connection.md §3.1](`../docs/connection.md`)).

use net::{
    deliver, send_direct, Category, Connection, Listener, NoRelay, NodeKey, Path, Relay,
    SignedMessage, TransportConfig, TransportError,
};
use std::sync::Mutex;
use std::time::Duration;

fn key() -> NodeKey {
    NodeKey::generate().expect("key")
}

fn message(key: &NodeKey, body: serde_json::Value) -> SignedMessage {
    SignedMessage::sign(key, "dev-a", "dev-b", 1_700_000_000_000, body).expect("sign")
}

/// A configuration with short timeouts, so a test that waits for one does not wait long.
fn quick() -> TransportConfig {
    TransportConfig {
        connect_timeout: Duration::from_millis(500),
        read_timeout: Duration::from_millis(500),
        write_timeout: Duration::from_millis(500),
        ..TransportConfig::default()
    }
}

/// The relay seam, wired for a test: it remembers every frame it was handed.
#[derive(Default)]
struct RecordingRelay {
    frames: Mutex<Vec<String>>,
}

impl RecordingRelay {
    fn frames(&self) -> Vec<String> {
        self.frames.lock().expect("lock").clone()
    }
}

impl Relay for RecordingRelay {
    fn forward(&self, frame: &str) -> Result<(), TransportError> {
        self.frames.lock().expect("lock").push(frame.to_string());
        Ok(())
    }

    fn describe(&self) -> String {
        "the recording relay".to_string()
    }
}

#[test]
fn a_message_round_trips_over_loopback_tcp() {
    let key = key();
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive().expect("receive")
    });

    let sent = message(&key, serde_json::json!({ "over": "tcp" }));
    let mut client = Connection::connect(addr).expect("connect");
    let written = client.send(&sent).expect("send");
    assert!(written > 0);

    let received = server.join().expect("thread");
    assert_eq!(received, sent, "the same message comes back");
}

#[test]
fn one_connection_carries_several_messages_one_line_each() {
    let key = key();
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        (0..3)
            .map(|_| connection.receive().expect("receive"))
            .collect::<Vec<_>>()
    });

    let mut client = Connection::connect(addr).expect("connect");
    let sent: Vec<SignedMessage> = (0..3)
        .map(|n| message(&key, serde_json::json!({ "n": n })))
        .collect();
    for one in &sent {
        client.send(one).expect("send");
    }

    let received = server.join().expect("thread");
    assert_eq!(received, sent, "three frames, in order, on one connection");
}

#[test]
fn the_frame_is_one_line_and_the_boundary_is_the_newline() {
    let key = key();
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive_frame().expect("frame")
    });

    let mut client = Connection::connect(addr).expect("connect");
    client
        .send(&message(&key, serde_json::json!({ "one": "line" })))
        .expect("send");

    let frame = server.join().expect("thread");
    assert!(
        frame.ends_with('\n'),
        "a frame ends with one newline: {frame:?}"
    );
    assert_eq!(
        frame.matches('\n').count(),
        1,
        "exactly one line: {frame:?}"
    );
    assert!(frame.starts_with(r#"{"v":1,"from":"dev-a""#), "{frame}");
}

#[test]
fn the_direct_path_is_tried_first_and_the_relay_only_carries_what_it_cannot() {
    let key = key();
    let sent = message(&key, serde_json::json!({ "route": "direct" }));

    // A listener that is dropped: nothing is listening on that port any more, so a
    // connect is refused.
    let dead = Listener::bind("127.0.0.1:0").expect("bind");
    let dead_addr = dead.local_addr().expect("addr");
    drop(dead);

    let relay = RecordingRelay::default();
    let path = deliver(dead_addr, &sent, Some(&relay), quick()).expect("delivered by relay");

    match path {
        Path::Relayed { after } => assert!(!after.is_empty(), "the direct failure is named"),
        other => panic!("expected the relay, got {other:?}"),
    }
    let frames = relay.frames();
    assert_eq!(frames.len(), 1, "the relay was asked exactly once");
    assert_eq!(
        frames[0],
        sent.to_line().expect("line"),
        "the relay was handed the frame itself, not a re-encoding"
    );
}

#[test]
fn with_nothing_wired_the_direct_failure_is_reported_rather_than_hidden() {
    let key = key();
    let sent = message(&key, serde_json::json!({}));
    let dead = Listener::bind("127.0.0.1:0").expect("bind");
    let dead_addr = dead.local_addr().expect("addr");
    drop(dead);

    let error = deliver(dead_addr, &sent, None, quick()).expect_err("nothing wired");
    assert!(matches!(error, TransportError::RelayUnavailable { .. }));
    assert_eq!(error.category(), Category::Network);

    // The same when a caller has a relay value that is deliberately empty.
    let error = deliver(dead_addr, &sent, Some(&NoRelay), quick()).expect_err("NoRelay");
    assert!(matches!(error, TransportError::RelayUnavailable { .. }));
}

#[test]
fn a_successful_direct_send_never_touches_the_relay() {
    let key = key();
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive().expect("receive")
    });

    let relay = RecordingRelay::default();
    let sent = message(&key, serde_json::json!({ "route": "direct" }));
    let path = deliver(addr, &sent, Some(&relay), quick()).expect("direct");
    assert_eq!(path, Path::Direct);
    assert!(relay.frames().is_empty(), "the relay carried nothing");

    let received = server.join().expect("thread");
    assert_eq!(received, sent);
}

#[test]
fn a_refused_connection_and_a_read_timeout_are_both_network() {
    let key = key();
    let sent = message(&key, serde_json::json!({}));

    // Refused: the port was bound and then released.
    let dead = Listener::bind("127.0.0.1:0").expect("bind");
    let dead_addr = dead.local_addr().expect("addr");
    drop(dead);
    let refused = send_direct(dead_addr, &sent, quick()).expect_err("refused");
    assert_eq!(
        refused.category(),
        Category::Network,
        "a refused connect is network: {refused}"
    );

    // Read timeout: connected, and nothing arrives.
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        // Accept and hold the connection open, well past the client's read timeout.
        let connection = listener.accept().expect("accept");
        std::thread::sleep(Duration::from_millis(1_200));
        drop(connection);
    });
    let mut client = Connection::connect_with(addr, quick()).expect("connect");
    let timeout = client.receive().expect_err("nothing to read");
    assert!(
        matches!(timeout, TransportError::Timeout { .. }),
        "expected a timeout, got {timeout:?}"
    );
    assert_eq!(timeout.category(), Category::Network);
    server.join().expect("thread");
}

#[test]
fn a_broken_frame_is_invalid_and_a_cut_short_one_is_network() {
    let wire = |bytes: &'static [u8]| {
        let listener = Listener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let server = std::thread::spawn(move || {
            let mut connection = listener.accept().expect("accept");
            connection.receive().map(|_| ()).map_err(|e| e.category())
        });
        let mut client = std::net::TcpStream::connect(addr).expect("connect");
        use std::io::Write;
        client.write_all(bytes).expect("write");
        drop(client);
        server.join().expect("thread")
    };

    // One complete line that is not a message: the input is wrong.
    assert_eq!(
        wire(b"not json at all\n"),
        Err(Category::Invalid),
        "a frame that does not parse is invalid"
    );
    // A line that never ends: the connection ended before a complete frame.
    assert_eq!(
        wire(b"{\"v\":1,\"from\":\"dev-a\""),
        Err(Category::Network),
        "a cut-short frame is network"
    );
    // A complete frame whose signature is not 64 bytes of base64url: still the input.
    assert_eq!(
        wire(
            b"{\"v\":1,\"from\":\"dev-a\",\"to\":\"dev-b\",\"ts\":1,\"body\":{},\"sig\":\"AA\"}\n"
        ),
        Err(Category::Invalid)
    );
}

#[test]
fn a_frame_longer_than_the_limit_is_refused_rather_than_read() {
    let listener = Listener::bind_with(
        "127.0.0.1:0",
        TransportConfig {
            max_frame_bytes: 64,
            ..quick()
        },
    )
    .expect("bind");
    let addr = listener.local_addr().expect("addr");

    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection
            .receive_frame()
            .map(|_| ())
            .map_err(|e| e.category())
    });

    let mut client = std::net::TcpStream::connect(addr).expect("connect");
    use std::io::Write;
    client.write_all(&[b'x'; 200]).expect("write");
    drop(client);

    assert_eq!(server.join().expect("thread"), Err(Category::Network));
}

#[test]
fn both_paths_carry_the_same_bytes() {
    let key = key();
    let sent = message(&key, serde_json::json!({ "identical": true }));

    // What the direct path puts on the wire…
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive_frame().expect("frame")
    });
    let mut client = Connection::connect(addr).expect("connect");
    client.send(&sent).expect("send");
    let on_the_wire = server.join().expect("thread");

    // …is what the relay is handed, character for character.
    let dead = Listener::bind("127.0.0.1:0").expect("bind");
    let dead_addr = dead.local_addr().expect("addr");
    drop(dead);
    let relay = RecordingRelay::default();
    deliver(dead_addr, &sent, Some(&relay), quick()).expect("relayed");

    assert_eq!(relay.frames(), vec![on_the_wire], "one encoder, two paths");
}
