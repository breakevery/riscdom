//! End-to-end smoke tests for the control-plane skeleton.
//!
//! These speak raw HTTP/1.1 over a TCP socket instead of using an HTTP client:
//! the wire format is the deliverable, and nothing here may call QEMU or the
//! network. The heartbeat is disabled so the streams under test are deterministic.

use host::AppState;
use host::EventSink;
use server::{
    Actor, ActorKind, AuthError, Authn, HttpEventSink, NoAuth, ReqMeta, Server, ServerConfig,
};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A workspace that no other test shares and that nothing cleans up (temp dir).
fn temp_workspace(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-server-{tag}-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp workspace");
    dir
}

/// Start a server on an ephemeral port and keep its runtime driven on a thread.
fn start_server(authn: Arc<dyn Authn>) -> (SocketAddr, HttpEventSink) {
    let workspace = temp_workspace("smoke");
    let app = Arc::new(AppState::in_memory(&workspace).expect("in-memory state"));
    let cfg = ServerConfig::new("127.0.0.1:0".parse().expect("addr"))
        .with_heartbeat(None)
        .with_authn(authn);
    let server = Server::new(Arc::clone(&app), cfg);
    let sink = server.sink(app.agent_id());

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .expect("runtime");
    let running = runtime.block_on(async { server.start().await.expect("bind") });
    let addr = running.local_addr();
    // Move the runtime to its own thread so the accept loop keeps being polled
    // while this (blocking) test thread talks to it. Dropping the runtime's task
    // handle detaches it; the process exits at the end of the test binary.
    std::thread::spawn(move || {
        runtime.block_on(std::future::pending::<()>());
    });
    (addr, sink)
}

/// Send a raw request and read until every needle has arrived, or time runs out.
fn request(addr: SocketAddr, raw: &str, needles: &[&str]) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream.write_all(raw.as_bytes()).expect("write request");
    read_until(&mut stream, needles)
}

fn read_until(stream: &mut TcpStream, needles: &[&str]) -> String {
    let mut collected = Vec::new();
    let mut buf = [0u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                collected.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&collected);
                if needles.iter().all(|needle| text.contains(needle)) {
                    break;
                }
            }
            Err(_) => break,
        }
        if Instant::now() > deadline {
            break;
        }
    }
    String::from_utf8_lossy(&collected).to_string()
}

fn get(path: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
}

#[test]
fn health_returns_200_and_json() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let response = request(addr, &get("/v0/health"), &["\"status\":\"ok\""]);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "got {response}");
    let lower = response.to_lowercase();
    assert!(
        lower.contains("content-type: application/json"),
        "got {response}"
    );
    assert!(response.contains("\"status\":\"ok\""), "got {response}");
    assert!(response.contains("\"version\":\""), "got {response}");
    assert!(response.contains("\"uptime_ms\":"), "got {response}");
}

#[test]
fn status_summarises_connections_subscribers_and_agents() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let response = request(addr, &get("/v0/status"), &["\"sse_subscribers\""]);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "got {response}");
    assert!(response.contains("\"sse_subscribers\":0"), "got {response}");
    assert!(response.contains("\"agents\":1"), "got {response}");
    assert!(response.contains("\"agent_id\":\""), "got {response}");
}

#[test]
fn the_event_stream_opens_with_a_hello_frame() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let response = request(addr, &get("/v0/events"), &["\"kind\":\"hello\""]);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "got {response}");
    let lower = response.to_lowercase();
    assert!(
        lower.contains("content-type: text/event-stream"),
        "got {response}"
    );
    assert!(lower.contains("cache-control: no-cache"), "got {response}");
    assert!(
        response.contains("data: {\"version\":1,\"kind\":\"hello\""),
        "the envelope's first key must be version: {response}"
    );
    assert!(response.contains("\nid: "), "got {response}");
}

#[test]
fn a_published_event_arrives_wrapped_in_the_envelope() {
    let (addr, sink) = start_server(Arc::new(NoAuth));
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
        .write_all(get("/v0/events").as_bytes())
        .expect("write request");

    // The `hello` frame is written only after the handler has subscribed, so once
    // it has been read, a publish cannot fall before the subscription and be lost.
    read_until(&mut stream, &["\"kind\":\"hello\""]);
    sink.emit(
        "agent:tool_call",
        serde_json::json!({ "name": "write_source" }),
    );

    let text = read_until(&mut stream, &["\"kind\":\"event\""]);
    assert!(text.contains("\"kind\":\"event\""), "got {text}");
    assert!(text.contains("\"event\":\"agent:tool_call\""), "got {text}");
    assert!(
        text.contains("\"payload\":{\"name\":\"write_source\"}"),
        "got {text}"
    );
    assert!(text.contains("\"agent_id\":\""), "got {text}");
}

#[test]
fn a_bad_bearer_token_is_refused_with_401() {
    let (addr, _sink) = start_server(Arc::new(TokenAuth { expected: "good" }));
    let bad = "GET /v0/health HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer nope\r\nConnection: close\r\n\r\n";
    let response = request(addr, bad, &["\"unauthorized\""]);
    assert!(response.starts_with("HTTP/1.1 401"), "got {response}");
    assert!(
        response.contains("\"code\":\"unauthorized\""),
        "got {response}"
    );
    assert!(response.contains("\"retryable\":false"), "got {response}");
}

#[test]
fn a_forbidden_actor_gets_403() {
    let (addr, _sink) = start_server(Arc::new(RefuseAll));
    let response = request(addr, &get("/v0/health"), &["\"forbidden\""]);
    assert!(response.starts_with("HTTP/1.1 403"), "got {response}");
    assert!(
        response.contains("\"code\":\"forbidden\""),
        "got {response}"
    );
}

#[test]
fn a_good_bearer_token_passes_through_the_same_hook() {
    let (addr, _sink) = start_server(Arc::new(TokenAuth { expected: "good" }));
    let ok = "GET /v0/health HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer good\r\nConnection: close\r\n\r\n";
    let response = request(addr, ok, &["\"status\":\"ok\""]);
    assert!(response.starts_with("HTTP/1.1 200 OK"), "got {response}");
}

#[test]
fn an_unknown_path_returns_the_error_model() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let response = request(addr, &get("/v0/nope"), &["\"not_found\""]);
    assert!(response.starts_with("HTTP/1.1 404"), "got {response}");
    assert!(
        response.contains("\"code\":\"not_found\""),
        "got {response}"
    );
    assert!(response.contains("\"message\":\""), "got {response}");
    assert!(response.contains("\"cause\":null"), "got {response}");
}

/// A test `Authn`: one token is good, everything else is refused.
struct TokenAuth {
    expected: &'static str,
}

impl Authn for TokenAuth {
    fn authorise(&self, meta: &ReqMeta) -> Result<Actor, AuthError> {
        if meta.token.as_deref() == Some(self.expected) {
            Ok(Actor {
                agent_id: "tester".to_string(),
                kind: ActorKind::Human,
            })
        } else {
            Err(AuthError::Unauthorized)
        }
    }
}

/// A test `Authn` that refuses everything, to exercise the 403 mapping.
struct RefuseAll;

impl Authn for RefuseAll {
    fn authorise(&self, _meta: &ReqMeta) -> Result<Actor, AuthError> {
        Err(AuthError::Forbidden)
    }
}
