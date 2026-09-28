//! The transport, proved offline: one JSON line per message over loopback TCP, direct
//! first and the relay only when direct fails (v1.0 M4a).
//!
//! ```text
//! cargo run -p net --example transport -- --self-test
//! ```
//!
//! Everything here is loopback and in-process: a listener on `127.0.0.1:0`, a thread on
//! the accepting side, and a relay stand-in that records what it was handed. No external
//! network, no server. `scripts/gate.sh` runs it beside the other example proofs.

use net::{
    deliver, send_direct, Category, Connection, Listener, NoRelay, NodeKey, Path, Relay,
    SignedMessage, TransportConfig, TransportError,
};
use std::sync::Mutex;
use std::time::Duration;

/// The relay seam, wired for the self-test: it remembers every frame it was handed.
#[derive(Default)]
struct RecordingRelay {
    frames: Mutex<Vec<String>>,
}

impl Relay for RecordingRelay {
    fn forward(&self, frame: &str) -> Result<(), TransportError> {
        self.frames.lock().expect("lock").push(frame.to_string());
        Ok(())
    }
}

fn check(what: &str, ok: bool, detail: String) {
    println!("{}  {what}: {detail}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}

fn quick() -> TransportConfig {
    TransportConfig {
        connect_timeout: Duration::from_millis(500),
        read_timeout: Duration::from_millis(500),
        write_timeout: Duration::from_millis(500),
        ..TransportConfig::default()
    }
}

/// A loopback address nobody is listening on: bound, then released.
fn dead_address() -> std::net::SocketAddr {
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    drop(listener);
    addr
}

fn self_test() {
    let key = NodeKey::generate().expect("key");
    let sign = |body: serde_json::Value| {
        SignedMessage::sign(&key, "dev-a", "dev-b", 1_700_000_000_000, body).expect("sign")
    };

    // 1. A message round-trips over loopback TCP.
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        (0..2)
            .map(|_| connection.receive().expect("receive"))
            .collect::<Vec<_>>()
    });
    let first = sign(serde_json::json!({ "n": 1 }));
    let second = sign(serde_json::json!({ "n": 2 }));
    let mut client = Connection::connect(addr).expect("connect");
    client.send(&first).expect("send");
    client.send(&second).expect("send");
    let received = server.join().expect("thread");
    check(
        "two messages, one connection, one JSON line each",
        received == vec![first.clone(), second.clone()],
        format!("{} frames", received.len()),
    );

    // 2. A frame is one line, and the newline is the boundary.
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive_frame().expect("frame")
    });
    let mut client = Connection::connect(addr).expect("connect");
    client.send(&first).expect("send");
    let frame = server.join().expect("thread");
    check(
        "a frame ends with exactly one newline",
        frame.ends_with('\n') && frame.matches('\n').count() == 1,
        frame.trim().to_string(),
    );

    // 3. Direct first: a live listener takes the message and the relay stays empty.
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive().expect("receive")
    });
    let relay = RecordingRelay::default();
    let path = deliver(addr, &first, Some(&relay), quick()).expect("direct");
    let received = server.join().expect("thread");
    check(
        "the direct path is tried first",
        path == Path::Direct && relay.frames.lock().expect("lock").is_empty() && received == first,
        "Path::Direct, relay untouched".to_string(),
    );

    // 4. When direct fails, the relay is asked — and handed the frame itself.
    let relay = RecordingRelay::default();
    let path = deliver(dead_address(), &first, Some(&relay), quick()).expect("relayed");
    let frames = relay.frames.lock().expect("lock").clone();
    check(
        "a failed direct send goes to the relay",
        matches!(path, Path::Relayed { .. }) && frames == vec![first.to_line().expect("line")],
        format!("{} frame(s)", frames.len()),
    );

    // 5. Both paths carry the same bytes: one encoder, two paths.
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive_frame().expect("frame")
    });
    let mut client = Connection::connect(addr).expect("connect");
    client.send(&second).expect("send");
    let on_the_wire = server.join().expect("thread");
    let relay = RecordingRelay::default();
    deliver(dead_address(), &second, Some(&relay), quick()).expect("relayed");
    let relayed = relay.frames.lock().expect("lock").clone();
    check(
        "the direct and relayed frames are byte-identical",
        relayed == vec![on_the_wire.clone()],
        on_the_wire.trim().to_string(),
    );

    // 6. With nothing wired, the direct failure is reported rather than hidden.
    let refused = deliver(dead_address(), &first, None, quick()).expect_err("nothing wired");
    check(
        "no relay wired is said out loud (network)",
        matches!(refused, TransportError::RelayUnavailable { .. })
            && refused.category() == Category::Network,
        refused.to_string(),
    );
    let shell = deliver(dead_address(), &first, Some(&NoRelay), quick()).expect_err("shell");
    check(
        "the relay shell answers 'not implemented' (network)",
        matches!(shell, TransportError::RelayUnavailable { .. })
            && shell.category() == Category::Network,
        shell.to_string(),
    );

    // 7. Failure mapping, as §3.1 freezes it.
    let refused = send_direct(dead_address(), &first, quick()).expect_err("refused");
    let refused_ok = refused.category() == Category::Network;

    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let connection = listener.accept().expect("accept");
        std::thread::sleep(Duration::from_millis(1_200));
        drop(connection);
    });
    let mut client = Connection::connect_with(addr, quick()).expect("connect");
    let timeout = client.receive().expect_err("nothing to read");
    let timeout_ok = matches!(timeout, TransportError::Timeout { .. })
        && timeout.category() == Category::Network;
    server.join().expect("thread");
    check(
        "a refused connect and a read timeout are both network",
        refused_ok && timeout_ok,
        format!("{} / {}", refused.category(), timeout.category()),
    );

    // 8. A complete frame that does not parse is the caller's input, not the transport.
    let listener = Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    let server = std::thread::spawn(move || {
        let mut connection = listener.accept().expect("accept");
        connection.receive().map(|_| ()).map_err(|e| e.category())
    });
    let mut client = std::net::TcpStream::connect(addr).expect("connect");
    use std::io::Write;
    client.write_all(b"not json at all\n").expect("write");
    drop(client);
    let broken = server.join().expect("thread");
    check(
        "a frame that does not parse is invalid",
        broken == Err(Category::Invalid),
        format!("{broken:?}"),
    );

    println!("net transport self-test: OK");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        self_test();
        return;
    }
    println!("usage: cargo run -p net --example transport -- --self-test");
    println!("moves signed messages over loopback TCP and proves the direct-then-relay rule.");
}
