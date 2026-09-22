//! End-to-end tests for the control plane: the query surface, the error model,
//! and the event stream.
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
///
/// It carries one source file, so `/v0/workspace/file` has something to read.
fn temp_workspace(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-server-{tag}-{}-{unique}",
        std::process::id()
    ));
    let src = dir.join("src");
    std::fs::create_dir_all(&src).expect("temp workspace");
    std::fs::write(src.join("main.c"), "int main(void) { return 0; }\n").expect("fixture");
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

/// One request, read to the end of the response (every request asks to close).
fn call(addr: SocketAddr, method: &str, path: &str, credential: Option<&str>) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    let header = match credential {
        Some(value) => format!("Authorization: Bearer {value}\r\n"),
        None => String::new(),
    };
    let request =
        format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\n{header}Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default();
    (status, body)
}

/// A `GET` whose body must parse as JSON.
fn get(addr: SocketAddr, path: &str) -> (u16, serde_json::Value) {
    let (status, body) = call(addr, "GET", path, None);
    let json =
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{path} is JSON ({e}): {body}"));
    (status, json)
}

#[test]
fn health_returns_200_and_json() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/health");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string(), "{body}");
    assert!(body["uptime_ms"].is_number(), "{body}");
}

#[test]
fn status_summarises_connections_subscribers_and_agents() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/status");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["sse_subscribers"], 0);
    assert_eq!(body["agents"], 1);
    assert!(body["agent_id"].is_string(), "{body}");
}

#[test]
fn every_query_endpoint_answers() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    // (path, expected status, a key the body must carry)
    let cases: &[(&str, u16, &str)] = &[
        ("/v0/audit/status", 200, "count"),
        ("/v0/audit/events?limit=5", 200, ""),
        ("/v0/runs?limit=5", 200, ""),
        ("/v0/runs/run-1-1", 200, ""),
        ("/v0/runs/diff?run_a=a&run_b=b", 500, "code"),
        ("/v0/llm/provider-presets", 200, ""),
        ("/v0/llm/config", 200, "configured"),
        ("/v0/llm/readiness", 200, "ready"),
        ("/v0/llm/local-probe", 200, "found"),
        ("/v0/llm/stored-key?provider_id=local", 200, "present"),
        ("/v0/sessions?limit=5", 200, ""),
        ("/v0/sessions/current", 200, "session_id"),
        ("/v0/snapshots", 200, ""),
        ("/v0/vm/running", 200, "running"),
        ("/v0/vm/status", 200, "running"),
        ("/v0/toolchain", 200, "found"),
        ("/v0/toolchain/download", 200, "in_progress"),
        ("/v0/qemu", 200, "found"),
        ("/v0/qemu/status", 200, "found"),
        ("/v0/preflight", 200, "rows"),
        ("/v0/settings/theme", 200, "theme"),
        ("/v0/settings/language", 200, "language"),
        ("/v0/workspace/root", 200, "root"),
        ("/v0/workspace/files", 200, ""),
        ("/v0/workspace/file?path=src%2Fmain.c", 200, "content"),
        ("/v0/serial", 200, "buffer"),
        // Reserved: served, and answers 501 until the aggregate lands.
        ("/v0/resources", 501, "code"),
    ];
    assert_eq!(cases.len(), 27, "26 queries plus the reserved aggregate");
    for (path, want_status, key) in cases {
        let (status, body) = get(addr, path);
        assert_eq!(status, *want_status, "{path}: {body}");
        if !key.is_empty() {
            assert!(body.get(key).is_some(), "{path} must carry {key}: {body}");
        }
    }
}

#[test]
fn a_missing_required_parameter_is_400() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/audit/events");
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "bad_request");
    assert_eq!(body["retryable"], false);
    assert_eq!(body["cause"], "limit");

    // And an unparsable one.
    let (status, body) = get(addr, "/v0/sessions?limit=lots");
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["cause"], "limit");
}

#[test]
fn the_reserved_aggregate_says_so() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/resources");
    assert_eq!(status, 501, "{body}");
    assert_eq!(body["code"], "not_implemented");
    assert_eq!(body["cause"], "resources");
}

#[test]
fn the_event_stream_opens_with_a_hello_frame() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let (status, body) = call(addr, "GET", "/v0/events", None);
    // The stream never ends, so read only as much as the hello frame needs.
    assert_eq!(status, 200);
    assert!(body.contains("\"kind\":\"hello\""), "{body}");
}

#[test]
fn the_stream_headers_and_frames_are_the_documented_ones() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
        .write_all(b"GET /v0/events HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("write request");
    let text = read_until(&mut stream, &["\"kind\":\"hello\""]);
    let lower = text.to_lowercase();
    assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
    assert!(lower.contains("content-type: text/event-stream"), "{text}");
    assert!(lower.contains("cache-control: no-cache"), "{text}");
    assert!(
        text.contains("data: {\"version\":1,\"kind\":\"hello\""),
        "the envelope's first key must be version: {text}"
    );
    assert!(text.contains("\nid: "), "{text}");
}

#[test]
fn a_published_event_arrives_wrapped_in_the_envelope() {
    let (addr, sink) = start_server(Arc::new(NoAuth));
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    stream
        .write_all(b"GET /v0/events HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .expect("write request");

    // The `hello` frame is written only after the handler has subscribed, so once
    // it has been read, a publish cannot fall before the subscription and be lost.
    read_until(&mut stream, &["\"kind\":\"hello\""]);
    sink.emit(
        "agent:tool_call",
        serde_json::json!({ "name": "write_source" }),
    );

    let text = read_until(&mut stream, &["\"kind\":\"event\""]);
    assert!(text.contains("\"kind\":\"event\""), "{text}");
    assert!(text.contains("\"event\":\"agent:tool_call\""), "{text}");
    assert!(
        text.contains("\"payload\":{\"name\":\"write_source\"}"),
        "{text}"
    );
    assert!(text.contains("\"agent_id\":\""), "{text}");
    assert!(text.contains("\"task_id\":null"), "{text}");
    assert!(text.contains("\"ts\":"), "{text}");
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

#[test]
fn a_bad_credential_is_refused_with_401() {
    let (addr, _sink) = start_server(Arc::new(TokenAuth { expected: "good" }));
    let (status, body) = call(addr, "GET", "/v0/health", Some("bad"));
    assert_eq!(status, 401, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(json["code"], "unauthorized");
    assert_eq!(json["retryable"], false);
}

#[test]
fn a_good_credential_passes_through_the_same_hook() {
    let (addr, _sink) = start_server(Arc::new(TokenAuth { expected: "good" }));
    let (status, body) = call(addr, "GET", "/v0/health", Some("good"));
    assert_eq!(status, 200, "{body}");
}

#[test]
fn a_forbidden_actor_gets_403() {
    let (addr, _sink) = start_server(Arc::new(RefuseAll));
    let (status, body) = get(addr, "/v0/health");
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
}

#[test]
fn an_unknown_path_returns_the_error_model() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/nope");
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["retryable"], false);
    assert_eq!(body["cause"], serde_json::Value::Null);
}

#[test]
fn a_known_path_under_the_wrong_method_is_405() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    let (status, body) = call(addr, "POST", "/v0/snapshots", None);
    assert_eq!(status, 405, "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).expect("JSON");
    assert_eq!(json["code"], "method_not_allowed");
    assert!(json["message"]
        .as_str()
        .unwrap_or_default()
        .contains("use GET"));

    // The host-local endpoints take part in the same rule.
    let (status, body) = call(addr, "DELETE", "/v0/events", None);
    assert_eq!(status, 405, "{body}");
    let (status, _) = call(addr, "POST", "/v0/health", None);
    assert_eq!(status, 405);
}

#[test]
fn the_capability_of_each_route_reaches_the_hook() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (addr, _sink) = start_server(Arc::new(CapabilitySpy {
        seen: Arc::clone(&seen),
        denied: "audit.read",
    }));
    // Allowed capability: 200.
    let (status, _) = get(addr, "/v0/runs?limit=1");
    assert_eq!(status, 200);
    // Denied capability: 403, and the hook saw which one it was.
    let (status, body) = get(addr, "/v0/audit/status");
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
    // The host-local endpoints carry theirs too.
    let (status, _) = get(addr, "/v0/health");
    assert_eq!(status, 200);
    let seen = seen.lock().expect("lock").clone();
    assert!(seen.contains(&"runs.read".to_string()), "{seen:?}");
    assert!(seen.contains(&"audit.read".to_string()), "{seen:?}");
    assert!(seen.contains(&"health.read".to_string()), "{seen:?}");
}

#[test]
fn the_body_carries_no_secret() {
    let (addr, _sink) = start_server(Arc::new(NoAuth));
    // The whole LLM status, which is where a key would leak if anywhere did.
    let (status, body) = call(addr, "GET", "/v0/llm/config", None);
    assert_eq!(status, 200, "{body}");
    assert!(!body.contains("api_key"), "{body}");
    assert!(!body.contains("sk-"), "{body}");
}

/// A test `Authn`: one credential is good, everything else is refused.
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

/// A test `Authn` that records the capability of every request and refuses one.
struct CapabilitySpy {
    seen: Arc<std::sync::Mutex<Vec<String>>>,
    denied: &'static str,
}

impl Authn for CapabilitySpy {
    fn authorise(&self, meta: &ReqMeta) -> Result<Actor, AuthError> {
        let capability = meta.capability.clone().unwrap_or_default();
        if let Ok(mut seen) = self.seen.lock() {
            seen.push(capability.clone());
        }
        if capability == self.denied {
            return Err(AuthError::Forbidden);
        }
        Ok(Actor {
            agent_id: "spy".to_string(),
            kind: ActorKind::Supervisor,
        })
    }
}
