//! End-to-end tests for the control plane: the query and control surfaces, the
//! error model, the event stream (with replay), and the token.
//!
//! These speak raw HTTP/1.1 over a TCP socket instead of using an HTTP client:
//! the wire format is the deliverable, and nothing here may call QEMU or the
//! network. The heartbeat is disabled so the streams under test are deterministic.

use host_core::AppState;
use host_core::EventSink;
use server::{
    Authn, Capability, HttpEventSink, NoAuth, Server, ServerConfig, TokenAuth, MAX_IMPORT_BYTES,
    REPLAY_CAPACITY,
};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
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
    let (addr, first, second) = serve(workspace.clone(), authn);
    (addr, first, second, workspace)
}

/// Serve one workspace on an ephemeral port, keeping the runtime driven on a
/// thread. Split out of [`start_server_with_two_sinks`] so a test can seed the
/// workspace (a hand-written sandbox definition, say) before the host reads it.
fn serve(workspace: PathBuf, authn: Arc<dyn Authn>) -> (SocketAddr, HttpEventSink, HttpEventSink) {
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
    (addr, first, second)
}

/// `serve`, and hand the state back so a test can arm the model (v0.9 sandbox F2d).
///
/// A test that wants the run to get *past* the readiness gate — to reach the
/// sandbox resolution behind it — needs a configured LLM, and only the state can
/// be told about one. No model call is made: the definition under test refuses the
/// run first.
fn serve_with_state(workspace: PathBuf, authn: Arc<dyn Authn>) -> (SocketAddr, Arc<AppState>) {
    let app = Arc::new(AppState::in_memory(&workspace).expect("in-memory state"));
    let cfg = ServerConfig::new("127.0.0.1:0".parse().expect("addr"))
        .with_heartbeat(None)
        .with_authn(authn);
    let server = Server::new(Arc::clone(&app), cfg);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .expect("runtime");
    let running = runtime.block_on(async { server.start().await.expect("bind") });
    let addr = running.local_addr();
    std::thread::spawn(move || {
        runtime.block_on(std::future::pending::<()>());
    });
    (addr, app)
}

/// Write `settings.json` into a workspace, before a state reads it.
fn write_settings(workspace: &Path, settings: serde_json::Value) {
    let dir = workspace.join(".riscdom");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("settings.json"),
        serde_json::to_string_pretty(&settings).expect("serde"),
    )
    .expect("settings");
}

/// A runnable binary that is not the tool it pretends to be (`<path> --version`
/// exits 0), the same stand-in `tests/qemu_injection.rs` uses.
fn a_runnable_binary(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    #[cfg(target_os = "windows")]
    {
        std::fs::copy(r"C:\Windows\System32\cmd.exe", &path).expect("copy cmd.exe");
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::copy("/bin/echo", &path).expect("copy echo");
    }
    path
}

/// One request, read to the end of the response (every request asks to close).
fn call(
    addr: SocketAddr,
    method: &str,
    path: &str,
    credential: &str,
    body: Option<&str>,
) -> (u16, String) {
    call_with(
        addr,
        method,
        path,
        credential,
        body,
        Duration::from_secs(20),
    )
}

/// The same, with the read timeout spelled out.
///
/// A sandbox switch that has to fail through three start attempts takes longer
/// than the default: the answer is the point of the test, so it waits for it.
fn call_with(
    addr: SocketAddr,
    method: &str,
    path: &str,
    credential: &str,
    body: Option<&str>,
    read_timeout: Duration,
) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(read_timeout))
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

/// One request whose body is bytes, answered as bytes (v0.9 project in/out).
///
/// The JSON helpers cannot carry an archive: their body is a `&str` and their
/// header says `application/json`. This one is byte-for-byte on purpose — the wire
/// format is the deliverable, and what is tested is that an archive survives it.
fn post_bytes_with_headers(
    addr: SocketAddr,
    path: &str,
    content_type: &str,
    body: &[u8],
) -> (u16, String, Vec<u8>) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .expect("read timeout");
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).expect("write head");
    // A refusal can arrive while the body is still going out (the `413`), so a
    // write error here is not a failure: the answer is read either way.
    let _ = stream.write_all(body);
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    split_response(&raw)
}

/// A `POST` of bytes, answered as JSON (the import's success and error shapes).
fn post_bytes_json(
    addr: SocketAddr,
    path: &str,
    content_type: &str,
    body: &[u8],
) -> (u16, serde_json::Value) {
    let (status, _headers, bytes) = post_bytes_with_headers(addr, path, content_type, body);
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("{path} is JSON ({e}): {}", String::from_utf8_lossy(&bytes)));
    (status, json)
}

/// Split a raw HTTP/1.1 response into status, lowercased headers and body.
fn split_response(raw: &[u8]) -> (u16, String, Vec<u8>) {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .unwrap_or(raw.len());
    let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (status, head, raw[split..].to_vec())
}

/// A plain tar with the given `(name, contents)` entries, built byte by byte.
///
/// Hand-built for the same reason the endpoint is tested with raw bytes: it needs
/// no dependency this test crate does not have, and it can name an entry the `tar`
/// crate would refuse to write (which is what a hostile upload looks like).
fn a_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, content) in entries {
        let mut header = [0u8; 512];
        let name_bytes = name.as_bytes();
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        let octal = |value: u64, width: usize| format!("{:0>width$o}\0", value, width = width - 1);
        header[100..108].copy_from_slice(octal(0o644, 8).as_bytes());
        header[108..116].copy_from_slice(octal(0, 8).as_bytes());
        header[116..124].copy_from_slice(octal(0, 8).as_bytes());
        header[124..136].copy_from_slice(octal(content.len() as u64, 12).as_bytes());
        header[136..148].copy_from_slice(octal(0, 12).as_bytes());
        header[148..156].copy_from_slice(b"        ");
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|b| *b as u32).sum();
        header[148..156].copy_from_slice(format!("{:06o}\0 ", checksum).as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(content);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.extend_from_slice(&[0u8; 1024]);
    out
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

/// A `POST` that is allowed to take longer than the default read timeout.
fn post_patient(addr: SocketAddr, path: &str, body: &str) -> (u16, serde_json::Value) {
    let (status, raw) = call_with(addr, "POST", path, "", Some(body), Duration::from_secs(90));
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
        ("/v0/sandboxes", 200, "sandboxes"),
        ("/v0/sandboxes/current", 200, "current"),
        ("/v0/sandboxes/candidates", 200, "toolchains"),
        ("/v0/sandboxes/default", 200, "name"),
        // The request queue (v0.9 sandbox F2c): empty until someone asks.
        ("/v0/sandboxes/requests", 200, "requests"),
        // Reserved: served, and answers 501 until the aggregate lands.
        ("/v0/resources", 501, "code"),
    ];
    assert_eq!(
        cases.len(),
        33,
        "31 query rows, the two path-parameter queries, and the reserved aggregate"
    );
    for (path, want_status, key) in cases {
        let (status, body) = get(addr, path);
        assert_eq!(status, *want_status, "{path}: {body}");
        if !key.is_empty() {
            assert!(body.get(key).is_some(), "{path} must carry {key}: {body}");
        }
    }
}

#[test]
fn the_sandbox_registry_answers_and_the_fallback_is_always_in_it() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));

    // A fresh machine: nothing hand-written, nothing installed, and the built-in
    // fallback still names a usable definition.
    let (status, body) = get(addr, "/v0/sandboxes");
    assert_eq!(status, 200, "{body}");
    let rows = body["sandboxes"].as_array().expect("an array of views");
    assert!(!rows.is_empty(), "{body}");
    assert_eq!(body["default"], "default", "{body}");
    assert!(body["current"].is_null(), "nothing is stored: {body}");
    let fallback = rows.last().expect("an entry");
    assert_eq!(fallback["name"], "default");
    assert_eq!(fallback["source"], "discovered");
    assert_eq!(fallback["shadowed"], false);
    assert!(fallback["runnable"].is_boolean(), "{fallback}");
    assert!(fallback["memory_mb"].is_null(), "{fallback}");

    // The current/default pair on its own.
    let (status, body) = get(addr, "/v0/sandboxes/current");
    assert_eq!(status, 200, "{body}");
    assert!(body["current"].is_null(), "{body}");
    assert_eq!(body["default"], "default", "{body}");

    // The raw scan, in the two independent lists the definition layer keeps.
    let (status, body) = get(addr, "/v0/sandboxes/candidates");
    assert_eq!(status, 200, "{body}");
    assert!(body["toolchains"].is_array(), "{body}");
    assert!(body["qemus"].is_array(), "{body}");
    // The scan is not the registry, and nothing was written back.
    assert!(
        body.get("sandboxes").is_none(),
        "the scan is not the merged list: {body}"
    );
}

#[test]
fn a_sandbox_name_is_a_path_parameter_and_a_literal_sub_path_is_not() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));

    // The fallback resolves by name on any machine.
    let (status, body) = get(addr, "/v0/sandboxes/default");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["name"], "default");

    // An unknown name is a `404` that names the parameter it could not use.
    let (status, body) = get(addr, "/v0/sandboxes/no-such-sandbox");
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["cause"], "name");

    // The literal sub-paths are their own routes — served, or reserved for the
    // rest of the F2 line — and never a definition that happens to be called
    // `current`, `switch` or `assemble`.
    for (path, want) in [
        ("/v0/sandboxes/current", 200),
        ("/v0/sandboxes/candidates", 200),
        // `switch` is a route of its own now (v0.9 sandbox F2b-2): `POST`-only, so
        // a `GET` is a `405` and never a definition that happens to be called that.
        ("/v0/sandboxes/switch", 405),
        ("/v0/sandboxes/assemble", 404),
        // `requests` is served since F2c, so it is no longer a 404 here.
        ("/v0/sandboxes/requests", 200),
    ] {
        let (status, body) = get(addr, path);
        assert_eq!(status, want, "{path}: {body}");
    }

    // A name never spans a slash, and the path serves `GET` only.
    let (status, _body) = get(addr, "/v0/sandboxes/a/b");
    assert_eq!(status, 404);
    let (status, _) = call(addr, "POST", "/v0/sandboxes/default", "", Some("{}"));
    assert_eq!(status, 405);
}

#[test]
fn a_sandbox_read_without_the_capability_is_403() {
    let (addr, _sink, _ws) = start_server(Arc::new(RefuseAll));
    let (status, body) = get(addr, "/v0/sandboxes");
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
}

#[test]
fn a_sandbox_switch_without_the_capability_is_403() {
    // The write next to those reads: refused by the server before the handler runs
    // (v0.9 sandbox F2b-2).
    let (addr, _sink, _ws) = start_server(Arc::new(RefuseAll));
    let (status, body) = post(addr, "/v0/sandboxes/switch", r#"{"name":"default"}"#);
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
}

#[test]
fn an_import_that_carries_the_hosts_state_is_a_400() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let archive = a_tar(&[(".riscdom/audit.db", b"pwned")]);
    let (status, body) =
        post_bytes_json(addr, "/v0/workspace/import", "application/x-tar", &archive);
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "bad_request");
    assert_eq!(body["cause"], "archive");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains(".riscdom"),
        "the message names the entry: {body}"
    );
}

#[test]
fn an_import_of_rubbish_is_a_400() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    // A body that is neither a zip nor a gzip stream nor a tar, whatever it claims
    // to be.
    for content_type in ["application/zip", "application/gzip", "application/x-tar"] {
        let (status, body) = post_bytes_json(
            addr,
            "/v0/workspace/import",
            content_type,
            b"not an archive at all",
        );
        assert_eq!(status, 400, "{content_type}: {body}");
        assert_eq!(body["cause"], "archive", "{content_type}: {body}");
    }
}

#[test]
fn an_import_without_the_capability_is_403() {
    // The one endpoint on the workspace surface that is a write, so the one that
    // needs `workspace.write` (v0.9 project in/out).
    let (addr, _sink, _ws) = start_server(Arc::new(RefuseAll));
    let archive = a_tar(&[("project.c", b"x")]);
    let (status, body) =
        post_bytes_json(addr, "/v0/workspace/import", "application/x-tar", &archive);
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
    // The hook refused, so there is no capability to name (`cause` is the
    // route-level refusal's business — see the narrowed-actor test below).

    let (status, body) = post(addr, "/v0/workspace/export", "{}");
    assert_eq!(status, 403, "{body}");
}

#[test]
fn a_holder_of_the_read_capability_cannot_import() {
    // The two workspace capabilities are not the same one: reading the project does
    // not let you replace it (v0.9 project in/out).
    let (addr, _sink, _ws) = start_server(Arc::new(FixedCaps {
        seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        held: vec![Capability::WorkspaceRead],
    }));
    let (status, body) = get(addr, "/v0/workspace/files");
    assert_eq!(status, 200, "the capability it holds: {body}");
    // It may export: that is reading.
    let (status, _headers, bytes) =
        post_bytes_with_headers(addr, "/v0/workspace/export", "application/json", b"{}");
    assert_eq!(status, 200, "export reads the workspace");
    assert_eq!(&bytes[..2], &[0x1f, 0x8b]);
    // It may not import.
    let archive = a_tar(&[("project.c", b"x")]);
    let (status, body) =
        post_bytes_json(addr, "/v0/workspace/import", "application/x-tar", &archive);
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
    assert_eq!(body["cause"], "capability", "{body}");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("workspace.write"),
        "the message names the capability: {body}"
    );
}

#[test]
fn an_import_larger_than_the_limit_is_a_413() {
    // Its own ceiling, not the shared 64 KiB one: a control body is a small JSON
    // object, a project archive is neither.
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let too_big = vec![0u8; MAX_IMPORT_BYTES + 4096];
    let (status, body) = post_bytes_json(addr, "/v0/workspace/import", "application/zip", &too_big);
    assert_eq!(status, 413, "{body}");
    assert_eq!(body["code"], "payload_too_large");
    assert_eq!(body["cause"], "body");
}

#[test]
fn the_project_leaves_and_comes_back() {
    let (addr, _sink, ws) = start_server(Arc::new(NoAuth));
    // The workspace has a project in it (the test writes it directly: the host's
    // own file-writing tool is the AI's, not the control plane's).
    std::fs::write(ws.join("project.c"), "int main(void){return 0;}\n").expect("write");

    // Out: bytes, not JSON, with the name a browser saves it under.
    let (status, headers, bytes) =
        post_bytes_with_headers(addr, "/v0/workspace/export", "application/json", b"{}");
    assert_eq!(status, 200, "{} bytes", bytes.len());
    assert!(
        headers.contains("content-type: application/gzip"),
        "{headers}"
    );
    assert!(
        headers.contains("content-disposition: attachment; filename=\"workspace.tar.gz\""),
        "{headers}"
    );
    assert_eq!(&bytes[..2], &[0x1f, 0x8b], "a gzip stream");

    // In: the same bytes, refused while the file is still there…
    let (status, body) = post_bytes_json(addr, "/v0/workspace/import", "application/gzip", &bytes);
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "conflict");
    assert_eq!(body["cause"], "exists");

    // …and accepted with `force`, which replaces it. Two files travel: the one the
    // test wrote and the one the smoke harness seeds (`src/main.c`).
    std::fs::write(ws.join("project.c"), "(replaced)\n").expect("write");
    let (status, body) = post_bytes_json(
        addr,
        "/v0/workspace/import?force=true",
        "application/gzip",
        &bytes,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["files"], 2, "{body}");
    assert!(body["bytes"].as_u64().unwrap_or(0) > 0, "{body}");
    assert_eq!(
        std::fs::read_to_string(ws.join("project.c")).expect("read"),
        "int main(void){return 0;}\n",
        "the import did not put the project back"
    );
}

#[test]
fn an_export_comes_back_through_an_import() {
    // The harness seeds `src/main.c`, so this is the small but complete round trip:
    // out as bytes, back through the archive reader, with the file intact.
    let (addr, _sink, ws) = start_server(Arc::new(NoAuth));
    let (status, _headers, bytes) =
        post_bytes_with_headers(addr, "/v0/workspace/export", "application/json", b"{}");
    assert_eq!(status, 200);
    assert_eq!(&bytes[..2], &[0x1f, 0x8b], "a gzip stream");

    // Take the seeded file away, then put the project back through the archive.
    std::fs::remove_dir_all(ws.join("src")).expect("clear the workspace");

    let (status, body) = post_bytes_json(addr, "/v0/workspace/import", "application/gzip", &bytes);
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["files"], 1, "{body}");
    assert_eq!(
        std::fs::read_to_string(ws.join("src").join("main.c")).expect("read"),
        "int main(void) { return 0; }\n",
        "the round trip lost or changed the file"
    );
}

#[test]
fn a_run_may_declare_its_sandbox_and_an_unknown_name_is_the_callers_404() {
    let ws = temp_workspace("run-sandbox");
    write_settings(
        &ws,
        serde_json::json!({
            "version": 1,
            "sandboxes": [{ "name": "blink", "memory_mb": 256 }],
            "default_sandbox": "blink",
        }),
    );
    let (addr, state) = serve_with_state(ws.clone(), Arc::new(NoAuth));
    // A configured model, so the run gets past the readiness gate — the sandbox
    // question is answered before any model call, and none is made.
    state.set_llm_config(host_core::LlmConfigInput {
        provider_id: "deepseek".to_string(),
        api_key: "test-key".to_string(),
        base_url: "http://127.0.0.1:9/".to_string(),
        model: "test-model".to_string(),
    });

    // An unknown name is the caller's parameter: `404`, naming it — not a run that
    // quietly uses some other sandbox.
    let (status, body) = post(
        addr,
        "/v0/agent/run",
        r#"{"user_input":"hi","sandbox":"no-such-sandbox"}"#,
    );
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["code"], "not_found");
    assert_eq!(body["cause"], "name");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("no-such-sandbox"),
        "the refusal names the name: {body}"
    );

    // A name that exists but cannot run is the *environment* (`503`, with the
    // reason code) — the same answer the switch gives a definition it cannot use.
    let missing = temp_workspace("run-sandbox-broken").join("no-such-qemu");
    write_settings(
        &ws,
        serde_json::json!({
            "version": 1,
            "sandboxes": [{ "name": "broken", "qemu_exe": missing.display().to_string() }],
            "default_sandbox": "broken",
        }),
    );
    // A second server: settings are read when the state is built.
    let (addr, state) = serve_with_state(ws, Arc::new(NoAuth));
    state.set_llm_config(host_core::LlmConfigInput {
        provider_id: "deepseek".to_string(),
        api_key: "test-key".to_string(),
        base_url: "http://127.0.0.1:9/".to_string(),
        model: "test-model".to_string(),
    });
    let (status, body) = post(
        addr,
        "/v0/agent/run",
        r#"{"user_input":"hi","sandbox":"broken"}"#,
    );
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["cause"], "sandbox_qemu_missing", "{body}");
    // The declaration never moved the node, and nothing is running.
    assert_eq!(state.current_sandbox(), None);
    assert_eq!(state.active_sandbox(), None);
}

#[test]
fn a_sandbox_request_without_the_capability_is_403() {
    // The queue's two faces, refused by the server before the handler runs
    // (v0.9 sandbox F2c): reading it needs `sandbox.read`, leaving an ask needs
    // `agent.run` — an actor that may run an agent may say what it wants.
    let (addr, _sink, _ws) = start_server(Arc::new(RefuseAll));
    let (status, body) = get(addr, "/v0/sandboxes/requests");
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
    let (status, body) = post(
        addr,
        "/v0/sandboxes/requests",
        r#"{"action":"switch","sandbox":"blink"}"#,
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
}

#[test]
fn an_ask_lands_in_the_queue_and_the_queue_reads_back() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));

    let (status, body) = get(addr, "/v0/sandboxes/requests");
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["requests"].as_array().map(Vec::len),
        Some(0),
        "nothing asked yet: {body}"
    );

    let (status, body) = post(
        addr,
        "/v0/sandboxes/requests",
        r#"{"action":"switch","sandbox":"blink","reason":"needs more memory"}"#,
    );
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_str().expect("an id").to_string();
    assert!(id.starts_with("req-"), "its own namespace: {id}");

    // The pending filter is the one an approver reads.
    let (status, body) = get(addr, "/v0/sandboxes/requests?status=pending");
    assert_eq!(status, 200, "{body}");
    let rows = body["requests"].as_array().expect("an array");
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(rows[0]["id"], serde_json::json!(id));
    assert_eq!(rows[0]["action"], "switch");
    assert_eq!(rows[0]["sandbox"], "blink");
    assert_eq!(rows[0]["reason"], "needs more memory");
    assert_eq!(rows[0]["status"], "pending");
    assert!(rows[0]["decided_by"].is_null());
    assert!(rows[0]["decided_at_ms"].is_null());
    assert!(rows[0]["requested_at_ms"].as_u64().unwrap_or(0) > 0);
    assert!(
        !rows[0]["requester_agent_id"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "the ask names who asked: {body}"
    );

    // A filter nobody knows is a `400`, not an empty queue.
    let (status, body) = get(addr, "/v0/sandboxes/requests?status=maybe");
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["code"], "bad_request");
    assert_eq!(body["cause"], "status");
}

#[test]
fn an_ask_without_a_usable_action_is_a_400() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    for (body, cause) in [(r#"{}"#, "action"), (r#"{"action":"reboot"}"#, "action")] {
        let (status, reply) = post(addr, "/v0/sandboxes/requests", body);
        assert_eq!(status, 400, "{body}: {reply}");
        assert_eq!(reply["code"], "bad_request");
        assert_eq!(reply["cause"], cause, "{body}: {reply}");
    }
    // Nothing reached the queue.
    let (status, body) = get(addr, "/v0/sandboxes/requests");
    assert_eq!(status, 200);
    assert_eq!(body["requests"].as_array().map(Vec::len), Some(0), "{body}");
}

#[test]
fn a_decision_needs_the_capability_the_request_asks_for() {
    // The gate is `sandbox.read` — a decider has to see the queue — and the
    // decision itself needs what the request's `action` implies. Here the actor
    // may switch, so a *switch* request goes through…
    let (addr, _sink, _ws) = start_server(Arc::new(FixedCaps {
        seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        held: vec![
            Capability::AgentRun,
            Capability::SandboxRead,
            Capability::SandboxSwitch,
        ],
    }));

    let (status, body) = post(
        addr,
        "/v0/sandboxes/requests",
        r#"{"action":"switch","sandbox":"blink"}"#,
    );
    assert_eq!(status, 201, "{body}");
    let switch_id = body["id"].as_str().expect("an id").to_string();

    let (status, body) = post(
        addr,
        &format!("/v0/sandboxes/requests/{switch_id}/approve"),
        "{}",
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "approved");
    assert!(!body["decided_by"].as_str().unwrap_or_default().is_empty());
    assert!(body["decided_at_ms"].as_u64().unwrap_or(0) > 0);

    // …while an *assemble* request does not: switching is one power, giving the
    // node a new definition to run is another (F2c decision 1).
    let (status, body) = post(
        addr,
        "/v0/sandboxes/requests",
        r#"{"action":"assemble","sandbox":"big"}"#,
    );
    assert_eq!(status, 201, "{body}");
    let assemble_id = body["id"].as_str().expect("an id").to_string();
    let (status, body) = post(
        addr,
        &format!("/v0/sandboxes/requests/{assemble_id}/approve"),
        "{}",
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["code"], "forbidden");
    assert_eq!(body["cause"], "capability");
    assert!(
        body["message"]
            .as_str()
            .unwrap_or_default()
            .contains("sandbox.assemble"),
        "the message names the capability it wanted: {body}"
    );

    // The refused decision changed nothing.
    let (status, body) = get(addr, "/v0/sandboxes/requests?status=pending");
    assert_eq!(status, 200, "{body}");
    let rows = body["requests"].as_array().expect("an array");
    assert_eq!(rows.len(), 1, "{body}");
    assert_eq!(rows[0]["id"], serde_json::json!(assemble_id));
}

#[test]
fn a_decision_is_final_and_an_unknown_id_is_a_404() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = post(
        addr,
        "/v0/sandboxes/requests",
        r#"{"action":"switch","sandbox":"blink"}"#,
    );
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_str().expect("an id").to_string();

    let (status, body) = post(addr, &format!("/v0/sandboxes/requests/{id}/reject"), "{}");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "rejected");

    // The second decision is a `409`: the first one stands.
    let (status, body) = post(addr, &format!("/v0/sandboxes/requests/{id}/approve"), "{}");
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["code"], "conflict");

    // And an id nobody knows is a `404` naming the id.
    for path in [
        "/v0/sandboxes/requests/req-0-404/approve",
        "/v0/sandboxes/requests/req-0-404/reject",
    ] {
        let (status, body) = post(addr, path, "{}");
        assert_eq!(status, 404, "{path}: {body}");
        assert_eq!(body["code"], "not_found");
        assert_eq!(body["cause"], "id");
    }
}

#[test]
fn a_request_is_never_the_thing_that_switches() {
    // Approving changes a record and nothing else (F2c decision 4): the running
    // sandbox is untouched, and the definition the request named need not even
    // exist — that is the switch's business, when someone calls it.
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (_, before) = get(addr, "/v0/sandboxes/current");
    let (status, body) = post(
        addr,
        "/v0/sandboxes/requests",
        r#"{"action":"switch","sandbox":"no-such-sandbox"}"#,
    );
    assert_eq!(status, 201, "{body}");
    let id = body["id"].as_str().expect("an id").to_string();
    let (status, body) = post(addr, &format!("/v0/sandboxes/requests/{id}/approve"), "{}");
    assert_eq!(status, 200, "{body}");
    let (_, after) = get(addr, "/v0/sandboxes/current");
    assert_eq!(before, after, "approving moved the node");
}

#[test]
fn the_sandbox_switch_answers_each_failure_with_its_own_status_and_cause() {
    // `404`: no definition by that name — the caller's parameter, the same answer
    // the name route next door gives.
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));
    let (status, body) = post(
        addr,
        "/v0/sandboxes/switch",
        r#"{"name":"no-such-sandbox"}"#,
    );
    assert_eq!(status, 404, "{body}");
    assert_eq!(body["cause"], "name");

    // `503`: a definition that cannot run — the environment, with the reason code
    // as `cause` so a client branches on a name.
    let workspace = temp_workspace("switch-validation");
    let missing = workspace.join("no-qemu-here");
    write_settings(
        &workspace,
        serde_json::json!({
            "version": 1,
            "sandboxes": [{ "name": "broken", "qemu_exe": missing.display().to_string() }]
        }),
    );
    let (addr, _first, _second) = serve(workspace, Arc::new(NoAuth));
    let (status, body) = post(addr, "/v0/sandboxes/switch", r#"{"name":"broken"}"#);
    assert_eq!(status, 503, "{body}");
    assert_eq!(body["cause"], "sandbox_qemu_missing");

    // `500`, and `409` while it runs: the stand-in is not QEMU, so the switch
    // passes every check and then spends three serial-connect timeouts failing to
    // start — which is the window a second request is refused in.
    let workspace = temp_workspace("switch-slow");
    let stand_in = a_runnable_binary(&workspace, "stand-in-qemu");
    let gcc = workspace.join("pinned-gcc");
    let kernel = workspace.join("guest.elf");
    std::fs::write(&gcc, b"not really a compiler").expect("write");
    std::fs::write(&kernel, b"not really a kernel").expect("write");
    write_settings(
        &workspace,
        serde_json::json!({
            "version": 1,
            "sandboxes": [{
                "name": "stand-in",
                "qemu_exe": stand_in.display().to_string(),
                "toolchain_path": gcc.display().to_string(),
                "kernel": kernel.display().to_string()
            }]
        }),
    );
    let (addr, _first, _second) = serve(workspace, Arc::new(NoAuth));
    let slow = std::thread::spawn(move || {
        post_patient(addr, "/v0/sandboxes/switch", r#"{"name":"stand-in"}"#)
    });
    // Long enough that the first switch is inside its start attempt (the slot is
    // claimed before any of that happens), short enough not to wait for it.
    std::thread::sleep(Duration::from_millis(700));
    let (status, body) = post(addr, "/v0/sandboxes/switch", r#"{"name":"stand-in"}"#);
    assert_eq!(status, 409, "{body}");
    assert_eq!(body["cause"], "sandbox");
    let (status, body) = slow.join().expect("the first switch answers");
    assert_eq!(status, 500, "{body}");
    assert_eq!(body["cause"], "sandbox_start_failed");
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
        // No executor is configured either, so every target is nobody's: the
        // dispatch is refused as the caller's parameter (v0.9 interface E0).
        (
            "/v0/tasks",
            r#"{"target":"executor-0","input":"hi"}"#.to_string(),
            404,
            "cause",
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
    // Every control the tests can answer has a case above. The exceptions are *named*, not
    // counted, so a control that lands without a case fails this test with the missing path
    // in the message — and the arithmetic lives in the route table rather than in a number
    // somebody has to remember to bump (v0.9 clean-up batch).
    //
    // Three reach outside the machine: `toolchain/download` fetches an archive,
    // `preflight/run` compiles and boots a guest, and `qemu/download` would fetch one if a
    // release were pinned. Four are owned end to end by their own tests: the sandbox
    // switch, the request queue's create, and project in/out. The two request decisions are
    // pattern routes rather than rows, so they are not in the table at all.
    let answered_elsewhere = [
        "/v0/toolchain/download",
        "/v0/preflight/run",
        "/v0/qemu/download",
        "/v0/sandboxes/switch",
        "/v0/sandboxes/requests",
        "/v0/workspace/import",
        "/v0/workspace/export",
    ];
    let covered: std::collections::BTreeSet<&str> = cases.iter().map(|(path, ..)| *path).collect();
    let expected: std::collections::BTreeSet<&str> = server::routes::control_paths()
        .into_iter()
        .filter(|path| !answered_elsewhere.contains(path))
        .collect();
    assert_eq!(
        covered, expected,
        "every control the tests can answer has a case"
    );

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
fn the_fleet_is_whatever_settings_json_says() {
    // The registry is configuration, not a runtime API: a test seeds the file the
    // host reads, which is why `serve` takes a workspace (v0.9 interface E0).
    let workspace = temp_workspace("executors");
    write_settings(
        &workspace,
        serde_json::json!({
            "version": 1,
            "executors": [
                { "label": "executor-0", "program": "worker.exe", "args": ["--workspace", "W"] },
                { "label": "executor-1", "program": "worker.exe" },
            ],
        }),
    );
    let (addr, _first, _second) = serve(workspace, Arc::new(NoAuth));

    let (status, body) = get(addr, "/v0/executors");
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["executors"],
        serde_json::json!([
            { "agent_id": "executor-0" },
            { "agent_id": "executor-1" },
        ])
    );
}

#[test]
fn a_node_with_no_fleet_has_none_and_refuses_every_target() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));

    let (status, body) = get(addr, "/v0/executors");
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["executors"], serde_json::json!([]));

    // The target is the caller's parameter, so a target nobody owns is a `404`
    // that says *which* parameter — not a `500` and not an empty answer.
    let (status, raw) = call(
        addr,
        "POST",
        "/v0/tasks",
        "",
        Some(r#"{"target":"executor-0","input":"say hi"}"#),
    );
    assert_eq!(status, 404, "{raw}");
    let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
    assert_eq!(json["code"], "not_found");
    assert_eq!(json["cause"], "target");
    assert!(
        json["message"]
            .as_str()
            .unwrap_or_default()
            .contains("executor-0"),
        "the message names the target: {raw}"
    );
}

#[test]
fn a_task_without_a_target_or_input_is_a_400_naming_it() {
    let (addr, _sink, _ws) = start_server(Arc::new(NoAuth));

    for (body, parameter) in [
        (r#"{"input":"say hi"}"#, "target"),
        (r#"{"target":"executor-0"}"#, "input"),
    ] {
        let (status, raw) = call(addr, "POST", "/v0/tasks", "", Some(body));
        assert_eq!(status, 400, "{body}: {raw}");
        let json: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
        assert_eq!(json["code"], "bad_request");
        assert_eq!(json["cause"], parameter, "{body}");
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
    for path in [
        "/v0/runs?limit=1",
        "/v0/audit/status",
        "/v0/health",
        "/v0/sandboxes",
        "/v0/executors",
    ] {
        let (status, body) = get(addr, path);
        assert_eq!(status, 200, "{path}: {body}");
    }
    // The switch route too: an unknown name answers `404`, and the hook still saw
    // what the route asked for.
    let (status, body) = post(addr, "/v0/sandboxes/switch", r#"{"name":"no-such"}"#);
    assert_eq!(status, 404, "{body}");
    // A dispatch to nobody is the same shape: the route asked for `agent.run`, and
    // the handler's own answer is what the caller sees.
    let (status, body) = post(addr, "/v0/tasks", r#"{"target":"nobody","input":"hi"}"#);
    assert_eq!(status, 404, "{body}");
    let (status, _) = call(addr, "POST", "/v0/sessions/clear", "", Some("{}"));
    assert_eq!(status, 204);

    let seen = seen.lock().expect("lock").clone();
    for expected in [
        Capability::RunsRead,
        Capability::AuditRead,
        Capability::HealthRead,
        Capability::SessionWrite,
        Capability::SandboxRead,
        Capability::SandboxSwitch,
        Capability::AgentRun,
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
        ("GET", "/v0/sandboxes", None),
        ("GET", "/v0/executors", None),
        ("POST", "/v0/sandboxes/switch", Some(r#"{"name":"any"}"#)),
        (
            "POST",
            "/v0/tasks",
            Some(r#"{"target":"any","input":"hi"}"#),
        ),
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
