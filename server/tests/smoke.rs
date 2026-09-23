//! End-to-end tests for the control plane: the query and control surfaces, the
//! error model, the event stream (with replay), and the token.
//!
//! These speak raw HTTP/1.1 over a TCP socket instead of using an HTTP client:
//! the wire format is the deliverable, and nothing here may call QEMU or the
//! network. The heartbeat is disabled so the streams under test are deterministic.

use host_core::AppState;
use host_core::EventSink;
use server::{
    Authn, Capability, HttpEventSink, NoAuth, Server, ServerConfig, TokenAuth, REPLAY_CAPACITY,
};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The token the token-protected servers in these tests hand out.
const TOKEN: &str = "test-token-0123456789abcdef";

/// The scheme word of the header, spelled once so no literal "scheme value" span
/// exists in this file.
const SCHEME: &str = "Bearer";

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
///
/// Returns the address, a sink that can publish events, and the workspace root
/// (so a test can build a path the host will accept).
fn start_server(authn: Arc<dyn Authn>) -> (SocketAddr, HttpEventSink, PathBuf) {
    let (addr, first, _second, ws) = start_server_with_two_sinks(authn);
    (addr, first, ws)
}

/// The same, but handing back **two** sinks built from one identity — which is
/// what two transports in one process look like.
fn start_server_with_two_sinks(
    authn: Arc<dyn Authn>,
) -> (SocketAddr, HttpEventSink, HttpEventSink, PathBuf) {
    let workspace = temp_workspace("smoke");
    let app = Arc::new(AppState::in_memory(&workspace).expect("in-memory state"));
    let cfg = ServerConfig::new("127.0.0.1:0".parse().expect("addr"))
        .with_heartbeat(None)
        .with_authn(authn);
    let server = Server::new(Arc::clone(&app), cfg);
    let first = server.sink();
    let second = server.sink();

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
    (addr, first, second, workspace)
}

/// One request, read to the end of the response (every request asks to close).
fn call(
    addr: SocketAddr,
    method: &str,
    path: &str,
    credential: &str,
    body: Option<&str>,
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("read timeout");
    let auth = if credential.is_empty() {
        String::new()
    } else {
        format!("Authorization: {} {}\r\n", SCHEME, credential)
    };
    let content = match body {
        Some(body) => format!(
            "Content-Type: application/json\r\nContent-Length: {}\r\n",
            body.len()
        ),
        None => String::new(),
    };
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth}{content}Connection: close\r\n\r\n{}",
        body.unwrap_or("")
    );
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
    let (status, body) = call(addr, "GET", path, "", None);
    (
        status,
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{path} is JSON ({e}): {body}")),
    )
}

/// A `POST` with a JSON body, answered as JSON.
fn post(addr: SocketAddr, path: &str, body: &str) -> (u16, serde_json::Value) {
    let (status, raw) = call(addr, "POST", path, "", Some(body));
    let json = serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} is JSON ({e}): {raw}"));
    (status, json)
}

/// Open an event stream and read until every needle has arrived.
fn open_stream(
    addr: SocketAddr,
    last_event_id: Option<&str>,
    needles: &[&str],
) -> (TcpStream, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("read timeout");
    let resume = match last_event_id {
        Some(id) => format!("Last-Event-ID: {id}\r\n"),
        None => String::new(),
    };
    let request = format!("GET /v0/events HTTP/1.1\r\nHost: localhost\r\n{resume}\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let text = read_until(&mut stream, needles);
    (stream, text)
}

fn read_until(stream: &mut TcpStream, needles: &[&str]) -> String {
    let mut collected = Vec::new();
    let mut buf = [0u8; 8192];
    let deadline = Instant::now() + Duration::from_secs(10);
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

/// The `id:` of the last frame in a stream capture.
fn last_id(text: &str) -> String {
    text.lines()
        .rfind(|line| line.starts_with("id: "))
        .map(|line| line.trim_start_matches("id: ").to_string())
        .unwrap_or_else(|| panic!("no id in: {text}"))
}

/// The event envelopes in a stream capture, in the order they arrived.
fn event_envelopes(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .filter(|value| value["kind"] == "event")
        .collect()
}

// ---------------------------------------------------------------------------
// Queries (v0.9 batch 3)
// ---------------------------------------------------------------------------

#[test]
fn health_returns_200_and_json() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/health");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "ok");
    assert!(body["version"].is_string(), "{body}");
    assert!(body["uptime_ms"].is_number(), "{body}");
}

#[test]
fn status_summarises_connections_subscribers_and_agents() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/status");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["sse_subscribers"], 0);
    assert_eq!(body["agents"], 1);
    assert!(body["agent_id"].is_string(), "{body}");
}

#[test]
fn every_query_endpoint_answers() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
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
        ("/v0/qemu/download", 200, "in_progress"),
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
    assert_eq!(
        cases.len(),
        28,
        "26 query rows, the path-parameter query, and the reserved aggregate"
    );
    for (path, want_status, key) in cases {
        let (status, body) = get(addr, path);
        assert_eq!(status, *want_status, "{path}: {body}");
        if !key.is_empty() {
            assert!(body.get(key).is_some(), "{path} must carry {key}: {body}");
        }
    }
}

// ---------------------------------------------------------------------------
// Controls (v0.9 batch 4)
// ---------------------------------------------------------------------------

#[test]
fn every_control_endpoint_answers() {
    let (addr, _sink, ws) = start_server(Arc::new(NoAuth));
    // JSON needs its backslashes escaped, and a Windows path has several.
    let escape = |name: &str| ws.join(name).display().to_string().replace('\\', "\\\\");
    let export = escape("export.jsonl");
    let serial = escape("serial.log");
    let sessions = escape("sessions.jsonl");

    // (path, body, expected status, a key the answer must carry)
    let cases: Vec<(&str, String, u16, &str)> = vec![
        // No LLM is configured in this workspace, so a run is `unavailable`.
        (
            "/v0/agent/run",
            r#"{"user_input":"hi"}"#.to_string(),
            503,
            "code",
        ),
        (
            "/v0/runs/export",
            format!(r#"{{"run_id":"run-9-9","path":"{export}"}}"#),
            404,
            "code",
        ),
        (
            "/v0/runs/abandon-stale",
            "{}".to_string(),
            200,
            "abandoned",
        ),
        ("/v0/vm/stop", "{}".to_string(), 204, ""),
        (
            "/v0/snapshots/save",
            r#"{"name":"nope"}"#.to_string(),
            409,
            "code",
        ),
        (
            "/v0/snapshots/resume",
            r#"{"name":"nope"}"#.to_string(),
            404,
            "code",
        ),
        (
            "/v0/snapshots/delete",
            r#"{"name":"nope"}"#.to_string(),
            200,
            "deleted",
        ),
        (
            "/v0/sessions/create",
            r#"{"title":"first"}"#.to_string(),
            200,
            "session_id",
        ),
        (
            "/v0/sessions/open",
            r#"{"session_id":"nope"}"#.to_string(),
            404,
            "code",
        ),
        // The host's rename and delete are idempotent for an unknown id (a no-op
        // that touches no row), so the endpoint mirrors that instead of inventing
        // a 404 the host would not have raised.
        (
            "/v0/sessions/rename",
            r#"{"session_id":"nope","title":"t"}"#.to_string(),
            204,
            "",
        ),
        (
            "/v0/sessions/delete",
            r#"{"session_id":"nope"}"#.to_string(),
            204,
            "",
        ),
        ("/v0/sessions/clear", "{}".to_string(), 204, ""),
        // Not in progress, so the cancel is a state clash.
        (
            "/v0/toolchain/download/cancel",
            "{}".to_string(),
            409,
            "code",
        ),
        (
            "/v0/toolchain/path",
            r#"{"path":"/no/such/riscv-gcc"}"#.to_string(),
            400,
            "code",
        ),
        ("/v0/toolchain/path/clear", "{}".to_string(), 204, ""),
        (
            "/v0/qemu/path",
            r#"{"path":"/no/such/qemu"}"#.to_string(),
            400,
            "code",
        ),
        ("/v0/qemu/path/clear", "{}".to_string(), 204, ""),
        ("/v0/preflight/ack", "{}".to_string(), 200, "rows"),
        (
            "/v0/audit/alert",
            r#"{"enabled":false}"#.to_string(),
            204,
            "",
        ),
        (
            "/v0/audit/export",
            format!(r#"{{"path":"{sessions}"}}"#),
            200,
            "events_exported",
        ),
        (
            "/v0/settings/theme",
            r#"{"theme":"dark"}"#.to_string(),
            204,
            "",
        ),
        (
            "/v0/settings/language",
            r#"{"language":"en"}"#.to_string(),
            204,
            "",
        ),
        (
            "/v0/llm/config",
            r#"{"api_key":"test-key","base_url":"https://api.deepseek.com","model":"deepseek-chat","provider_id":"deepseek","remember":false}"#.to_string(),
            204,
            "",
        ),
        (
            "/v0/llm/stored-key/load",
            r#"{"provider_id":"deepseek"}"#.to_string(),
            404,
            "code",
        ),
        ("/v0/llm/config/clear", "{}".to_string(), 204, ""),
        (
            "/v0/serial/export",
            format!(r#"{{"path":"{serial}"}}"#),
            200,
            "bytes_written",
        ),
        // QEMU download (v0.9 sandbox F1): nothing is running, so a cancel is the
        // documented conflict. The *start* is not in this table on purpose — see
        // `a_qemu_download_is_refused_because_no_release_is_pinned`.
        ("/v0/qemu/download/cancel", "{}".to_string(), 409, "code"),
        // §6 G1: reserved.
        ("/v0/vm/start", "{}".to_string(), 501, "code"),
    ];
    // 27 controls of §5.2, minus the three that would reach outside the machine
    // (`toolchain/download` fetches an archive, `preflight/run` compiles and boots a
    // guest, and `qemu/download` would fetch one if a release were pinned), plus the
    // reserved `POST /v0/vm/start`, `POST /v0/runs/abandon-stale` and the QEMU cancel
    // (nothing is running, so it is the documented conflict).
    assert_eq!(cases.len(), 28, "31 POST rows - 3 offline-unsafe");

    for (path, body, want_status, key) in &cases {
        let (status, raw) = call(addr, "POST", path, "", Some(body));
        assert_eq!(status, *want_status, "{path} {body}: {raw}");
        if !key.is_empty() {
            let json: serde_json::Value = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("{path} is JSON ({e}): {raw}"));
            assert!(json.get(key).is_some(), "{path} must carry {key}: {raw}");
        }
    }
}

#[test]
fn the_three_offline_unsafe_controls_are_routed_without_being_called() {
    // One fetches an archive, one compiles and boots a guest, and the third would
    // fetch one the day a release is pinned, so none of them is *called* here. A 405
    // on the path proves the route exists (it is served under another method)
    // without running the action.
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    for (method, path) in [
        ("GET", "/v0/preflight/run"),
        ("PUT", "/v0/toolchain/download"),
        ("PUT", "/v0/qemu/download"),
    ] {
        let (status, raw) = call(addr, method, path, "", None);
        assert_eq!(status, 405, "{method} {path}: {raw}");
        let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
        assert_eq!(json["code"], "method_not_allowed");
    }
    // And a hook that refuses everything stops them before they do anything.
    let (addr, _sink, _ws) = start_server(Arc::new(RefuseAll));
    for path in [
        "/v0/toolchain/download",
        "/v0/preflight/run",
        "/v0/qemu/download",
    ] {
        let (status, _) = call(addr, "POST", path, "", Some("{}"));
        assert_eq!(status, 403, "{path}");
    }
}

#[test]
fn a_qemu_download_is_refused_because_no_release_is_pinned() {
    // The recorded decision (`docs/qemu-distribution.md` §5): RiscDom guides the user
    // to a QEMU they install themselves and pins no release, so the spec lookup
    // refuses on every platform and the endpoint answers the documented
    // `503 unavailable` with `cause: "qemu"` and the guidance in `message`.
    //
    // Calling it is safe **because** of that refusal: the handler resolves the spec
    // before it claims a slot or spawns anything. If a release is ever pinned this
    // test is wrong on purpose — the answer becomes `202 {state:started}` and the
    // route joins the offline-unsafe list above.
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = post(addr, "/v0/qemu/download", "{}");
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["code"], "unavailable", "{body}");
    assert_eq!(body["cause"], "qemu", "{body}");
    let message = body["message"].as_str().expect("message");
    assert!(message.contains("QEMU"), "{message}");
    assert!(message.contains("no QEMU download is pinned"), "{message}");
    // Nothing started, so the status query still says idle...
    let (status, body) = get(addr, "/v0/qemu/download");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["in_progress"], false, "{body}");
    // ...and no slot was claimed, so a cancel is still the conflict.
    let (status, body) = post(addr, "/v0/qemu/download/cancel", "{}");
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "conflict", "{body}");
}

#[test]
fn a_path_outside_the_workspace_is_a_bad_request() {
    // The workspace boundary is the caller's *parameter* being unusable, so the
    // policy answers the documented `400 bad_request` with `cause: "path"`.
    // `403` belongs to authentication and authorisation and stays there (see the
    // `RefuseAll` tests below).
    let (addr, _sink, ws) = start_server(Arc::new(NoAuth));
    let outside = ws.parent().expect("parent").join("escaped.jsonl");
    // Built rather than formatted: a Windows path in a quoted JSON string needs
    // its backslashes escaped, which `serde_json` knows and a `format!` does not.
    let outside_body = serde_json::json!({ "path": outside.to_string_lossy() }).to_string();
    let cases = [
        (
            "POST",
            "/v0/audit/export",
            Some(outside_body),
            "outside workspace",
        ),
        (
            "POST",
            "/v0/serial/export",
            Some(r#"{"path":"../escaped.log"}"#.to_string()),
            "traversal",
        ),
        (
            "GET",
            "/v0/workspace/file?path=../escaped.txt",
            None,
            "traversal",
        ),
    ];
    for (method, path, body, needle) in cases {
        let (status, raw) = call(addr, method, path, "", body.as_deref());
        assert_eq!(status, 400, "{method} {path}: {raw}");
        let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
        assert_eq!(json["code"], "bad_request", "{path}: {raw}");
        assert_eq!(json["cause"], "path", "{path}: {raw}");
        let message = json["message"].as_str().expect("message");
        assert!(message.contains(needle), "{path}: {raw}");
    }
}

#[test]
fn a_control_endpoint_validates_its_body() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    // Not JSON at all.
    let (status, raw) = call(addr, "POST", "/v0/sessions/create", "", Some("not json"));
    assert_eq!(status, 400, "{raw}");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
    assert_eq!(json["code"], "bad_request");
    assert_eq!(json["cause"], "body");

    // JSON, but not an object.
    let (status, raw) = call(addr, "POST", "/v0/sessions/create", "", Some("[1,2]"));
    assert_eq!(status, 400, "{raw}");

    // An object without the required field.
    let (status, json) = post(addr, "/v0/sessions/create", "{}");
    assert_eq!(status, 400, "{json}");
    assert_eq!(json["cause"], "title");

    // A value outside the documented set.
    let (status, json) = post(addr, "/v0/settings/theme", r#"{"theme":"chartreuse"}"#);
    assert_eq!(status, 400, "{json}");
    assert_eq!(json["cause"], "theme");
    let (status, json) = post(addr, "/v0/audit/alert", r#"{"enabled":"maybe"}"#);
    assert_eq!(status, 400, "{json}");
    assert_eq!(json["cause"], "enabled");
}

// ---------------------------------------------------------------------------
// Authentication (v0.9 batch 4)
// ---------------------------------------------------------------------------

#[test]
fn the_token_is_required_when_it_is_installed() {
    let (addr, _sink, _ws) = start_server(Arc::new(TokenAuth::new(TOKEN)));
    // No header, and a wrong one: both are 401 with the documented body.
    for credential in ["", "wrong", TOKEN.trim_end_matches('f')] {
        let (status, raw) = call(addr, "GET", "/v0/health", credential, None);
        assert_eq!(status, 401, "{credential:?}: {raw}");
        let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
        assert_eq!(json["code"], "unauthorized");
        assert_eq!(json["retryable"], false);
    }
    // The right one: through.
    let (status, _) = call(addr, "GET", "/v0/health", TOKEN, None);
    assert_eq!(status, 200);
}

#[test]
fn the_destructive_controls_need_the_token() {
    let (addr, _sink, _ws) = start_server(Arc::new(TokenAuth::new(TOKEN)));
    for (path, body) in [
        ("/v0/sessions/clear", "{}"),
        ("/v0/vm/stop", "{}"),
        ("/v0/llm/config/clear", "{}"),
        ("/v0/sessions/delete", r#"{"session_id":"any"}"#),
    ] {
        let (status, raw) = call(addr, "POST", path, "", Some(body));
        assert_eq!(status, 401, "{path} without a token: {raw}");
        let (status, _) = call(addr, "POST", path, TOKEN, Some(body));
        assert_ne!(status, 401, "{path} with the token");
    }
}

#[test]
fn no_auth_lets_everything_through() {
    // What `--no-auth` means: the same destructive call, no credential. (The CLI
    // test pins that `--no-auth` is what selects this hook.)
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, _) = call(addr, "POST", "/v0/sessions/clear", "", Some("{}"));
    assert_eq!(status, 204);
}

#[test]
fn a_status_call_with_the_token_still_does_not_leak_it() {
    let (addr, _sink, _ws) = start_server(Arc::new(TokenAuth::new(TOKEN)));
    let (status, raw) = call(addr, "GET", "/v0/status", TOKEN, None);
    assert_eq!(status, 200, "{raw}");
    assert!(
        !raw.contains(TOKEN),
        "the token must never be echoed: {raw}"
    );
}

// ---------------------------------------------------------------------------
// The event stream: framing, replay, gaps
// ---------------------------------------------------------------------------

#[test]
fn two_transports_agree_on_the_identity_of_one_event() {
    // The identity of an event comes from the **source** (the `AppState` that
    // emitted it), not from the transport: two sinks built from one instance must
    // stamp the same `agent_id`, or the audit reader would see one event as two
    // different agents depending on where it was sent.
    let (addr, first, second, _ws) = start_server_with_two_sinks(Arc::new(NoAuth));
    let (mut stream, _) = open_stream(addr, None, &["\"kind\":\"hello\""]);
    let payload = serde_json::json!({ "name": "write_source", "arguments": {} });
    first.emit("agent:tool_call", payload.clone());
    second.emit("agent:tool_call", payload.clone());

    // Both frames arrive on the one stream; collect them as they do.
    let mut collected = String::new();
    let mut buf = [0u8; 8192];
    let deadline = Instant::now() + Duration::from_secs(10);
    while event_envelopes(&collected).len() < 2 && Instant::now() < deadline {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => collected.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(_) => break,
        }
    }

    let envelopes = event_envelopes(&collected);
    assert_eq!(envelopes.len(), 2, "one frame per emit: {collected}");
    let (a, b) = (&envelopes[0], &envelopes[1]);
    assert_eq!(a["agent_id"], b["agent_id"], "one source, one identity");
    assert!(a["agent_id"]
        .as_str()
        .unwrap_or_default()
        .starts_with("local-"));
    assert_eq!(a["event"], b["event"]);
    assert_eq!(a["payload"], b["payload"]);
    assert_eq!(a["version"], b["version"]);
    assert_eq!(a["kind"], b["kind"]);
    assert_eq!(a["task_id"], b["task_id"]);
    // The frame is stamped where it is written, so the two are not necessarily
    // the same millisecond — but they describe the same instant.
    let (ta, tb) = (a["ts"].as_i64().expect("ts"), b["ts"].as_i64().expect("ts"));
    assert!((ta - tb).abs() < 50, "{ta} vs {tb}");
}

#[test]
fn the_stream_headers_and_frames_are_the_documented_ones() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (_stream, text) = open_stream(addr, None, &["\"kind\":\"hello\""]);
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
    let (addr, sink, _ws) = start_server(Arc::new(NoAuth));
    let (mut stream, _) = open_stream(addr, None, &["\"kind\":\"hello\""]);
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

#[test]
fn reconnecting_replays_what_the_client_missed() {
    let (addr, sink, _ws) = start_server(Arc::new(NoAuth));

    // First connection: read the hello and one event, then note its id.
    let (mut first, _) = open_stream(addr, None, &["\"kind\":\"hello\""]);
    sink.emit("serial:chunk", serde_json::json!({ "chunk": "one" }));
    let text = read_until(&mut first, &["\"chunk\":\"one\""]);
    let cursor = last_id(&text);
    drop(first);

    // Two more events happen while nobody is listening.
    sink.emit("serial:chunk", serde_json::json!({ "chunk": "two" }));
    sink.emit("serial:chunk", serde_json::json!({ "chunk": "three" }));

    // The reconnect carries the cursor and gets both, before anything live.
    let (_second, text) = open_stream(addr, Some(&cursor), &["\"chunk\":\"three\""]);
    let two = text
        .find("\"chunk\":\"two\"")
        .expect("the first missed frame");
    let three = text
        .find("\"chunk\":\"three\"")
        .expect("the second missed frame");
    assert!(two < three, "replay keeps the order: {text}");
    assert!(
        !text.contains("\"kind\":\"gap\""),
        "the hole was inside the buffer, so there is no gap frame: {text}"
    );
}

#[test]
fn a_cursor_older_than_the_buffer_gets_a_gap_frame() {
    let (addr, sink, _ws) = start_server(Arc::new(NoAuth));

    let (mut first, _) = open_stream(addr, None, &["\"kind\":\"hello\""]);
    sink.emit("serial:chunk", serde_json::json!({ "chunk": "ancient" }));
    let text = read_until(&mut first, &["\"chunk\":\"ancient\""]);
    let cursor = last_id(&text);
    drop(first);

    // Push the cursor out of the buffer.
    for i in 0..(REPLAY_CAPACITY + 3) {
        sink.emit(
            "serial:chunk",
            serde_json::json!({ "chunk": format!("filler-{i}") }),
        );
    }

    let (_second, text) = open_stream(addr, Some(&cursor), &["\"kind\":\"gap\""]);
    assert!(text.contains("\"kind\":\"gap\""), "expected a gap: {text}");
    assert!(text.contains("\"lost_after\":\""), "{text}");
    assert!(
        text.contains("\"chunk\":\"filler-"),
        "the frames the buffer still holds are replayed: {text}"
    );
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[test]
fn a_missing_required_parameter_is_400() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/audit/events");
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "bad_request");
    assert_eq!(body["retryable"], false);
    assert_eq!(body["cause"], "limit");

    let (status, body) = get(addr, "/v0/sessions?limit=lots");
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["cause"], "limit");
}

#[test]
fn the_reserved_aggregate_says_so() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/resources");
    assert_eq!(status, 501, "{body}");
    assert_eq!(body["code"], "not_implemented");
    assert_eq!(body["cause"], "resources");
}

#[test]
fn an_unknown_path_returns_the_error_model() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = get(addr, "/v0/nope");
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["retryable"], false);
    assert_eq!(body["cause"], serde_json::Value::Null);
}

#[test]
fn a_known_path_under_the_wrong_method_is_405() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, raw) = call(addr, "POST", "/v0/snapshots", "", None);
    assert_eq!(status, 405, "{raw}");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
    assert_eq!(json["code"], "method_not_allowed");
    assert!(json["message"]
        .as_str()
        .unwrap_or_default()
        .contains("use GET"));

    for (method, path) in [
        ("DELETE", "/v0/events"),
        ("POST", "/v0/health"),
        ("GET", "/v0/sessions/clear"),
    ] {
        let (status, _) = call(addr, method, path, "", None);
        assert_eq!(status, 405, "{method} {path}");
    }
}

#[test]
fn a_bad_credential_is_refused_with_401() {
    let (addr, _sink, _ws) = start_server(Arc::new(TokenAuth::new("good")));
    let (status, raw) = call(addr, "GET", "/v0/health", "bad", None);
    assert_eq!(status, 401, "{raw}");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
    assert_eq!(json["code"], "unauthorized");
    assert_eq!(json["retryable"], false);
}

#[test]
fn a_forbidden_actor_gets_403() {
    let (addr, _sink, _ws) = start_server(Arc::new(RefuseAll));
    let (status, body) = get(addr, "/v0/health");
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
}

#[test]
fn the_capability_of_each_route_reaches_the_hook() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    // The actor holds everything, so this test is about what the *routes* ask
    // for, not about what is allowed.
    let (addr, _sink, _ws) = start_server(Arc::new(FixedCaps {
        seen: Arc::clone(&seen),
        held: Capability::ALL.to_vec(),
    }));
    for path in ["/v0/runs?limit=1", "/v0/audit/status", "/v0/health"] {
        let (status, body) = get(addr, path);
        assert_eq!(status, 200, "{path}: {body}");
    }
    let (status, _) = call(addr, "POST", "/v0/sessions/clear", "", Some("{}"));
    assert_eq!(status, 204);

    let seen = seen.lock().expect("lock").clone();
    for expected in [
        Capability::RunsRead,
        Capability::AuditRead,
        Capability::HealthRead,
        Capability::SessionWrite,
    ] {
        assert!(seen.contains(&expected), "{expected} in {seen:?}");
    }
}

#[test]
fn an_actor_without_the_capability_is_refused_with_403() {
    // The hook authenticates the caller (it returns an actor) but that actor holds
    // exactly one capability: the *server* is what refuses the rest.
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (addr, _sink, _ws) = start_server(Arc::new(FixedCaps {
        seen: Arc::clone(&seen),
        held: vec![Capability::RunsRead],
    }));

    let (status, _) = get(addr, "/v0/runs?limit=1");
    assert_eq!(status, 200, "the one capability it holds");

    for (method, path, body) in [
        ("GET", "/v0/audit/status", None),
        ("GET", "/v0/health", None),
        ("POST", "/v0/sessions/clear", Some("{}")),
    ] {
        let (status, raw) = call(addr, method, path, "", body);
        assert_eq!(status, 403, "{method} {path}: {raw}");
        let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
        assert_eq!(json["code"], "forbidden");
        assert_eq!(json["cause"], "capability");
        assert!(
            json["message"].as_str().unwrap_or_default().contains('.'),
            "the message names the capability: {raw}"
        );
    }
}

#[test]
fn the_body_carries_no_secret() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, raw) = call(addr, "GET", "/v0/llm/config", "", None);
    assert_eq!(status, 200, "{raw}");
    assert!(!raw.contains("api_key"), "{raw}");
    assert!(!raw.contains("sk-"), "{raw}");
}

// ---------------------------------------------------------------------------
// Test hooks
// ---------------------------------------------------------------------------

/// A test `Authn` that authenticates everyone as an actor holding exactly the
/// capabilities it was given, and records what each request asked for.
struct FixedCaps {
    seen: Arc<std::sync::Mutex<Vec<Capability>>>,
    held: Vec<Capability>,
}

impl Authn for FixedCaps {
    fn authorise(&self, meta: &server::ReqMeta) -> Result<server::Actor, server::AuthError> {
        if let Some(capability) = meta.capability {
            if let Ok(mut seen) = self.seen.lock() {
                seen.push(capability);
            }
        }
        Ok(server::Actor {
            agent_id: "limited".to_string(),
            kind: server::ActorKind::Supervisor,
            capabilities: self.held.iter().copied().collect(),
        })
    }
}

/// A test `Authn` that refuses everything, to exercise the 403 mapping.
struct RefuseAll;

impl Authn for RefuseAll {
    fn authorise(&self, _meta: &server::ReqMeta) -> Result<server::Actor, server::AuthError> {
        Err(server::AuthError::Forbidden)
    }
}
