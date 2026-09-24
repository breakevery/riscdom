//! The Web UI the control plane can serve (v0.9 D2a).
//!
//! These speak raw HTTP/1.1 over a TCP socket, like `smoke.rs`, because the wire
//! answer is the deliverable: the status, the `Content-Type` and what a client
//! cannot make the server read. The heartbeat is disabled so nothing is buffered
//! behind a timer.

use host_core::AppState;
use server::{Authn, NoAuth, Server, ServerConfig, TokenAuth};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A directory no other test shares (temp dir; nothing cleans it up).
fn temp_dir(tag: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-webui-{tag}-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

/// A built frontend, in the shape `vite build` leaves behind: one entry document
/// and one content-hashed asset under `assets/`.
///
/// Returns the directory, so a test can also write a decoy beside it.
fn built_frontend(tag: &str) -> PathBuf {
    let root = temp_dir(tag);
    std::fs::write(
        root.join("index.html"),
        "<!doctype html><div id=\"root\"></div>\n",
    )
    .expect("index.html");
    std::fs::create_dir_all(root.join("assets")).expect("assets dir");
    std::fs::write(
        root.join("assets").join("index-abc123.js"),
        "export const root = 1;\n",
    )
    .expect("asset");
    root
}

/// Start a server on an ephemeral port and keep its runtime driven on a thread.
fn serve(web_root: Option<PathBuf>, authn: Arc<dyn Authn>) -> SocketAddr {
    let workspace = temp_dir("ws");
    let app = Arc::new(AppState::in_memory(&workspace).expect("in-memory state"));
    let mut cfg = ServerConfig::new("127.0.0.1:0".parse().expect("addr"))
        .with_heartbeat(None)
        .with_authn(authn);
    if let Some(root) = web_root {
        cfg = cfg.with_web_root(root);
    }
    let server = Server::new(app, cfg);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .expect("runtime");
    let running = runtime.block_on(async { server.start().await.expect("bind") });
    let addr = running.local_addr();
    std::thread::spawn(move || {
        runtime.block_on(std::future::pending::<()>());
    });
    addr
}

/// One `GET`, answered as `(status, headers, body)` (the request asks to close).
fn get(
    addr: SocketAddr,
    path: &str,
    credential: Option<&str>,
) -> (u16, Vec<(String, String)>, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("read timeout");
    let auth = match credential {
        Some(token) => format!("Authorization: Bearer {token}\r\n"),
        None => String::new(),
    };
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{auth}Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("write request");
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_str(), ""));
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|line| {
            line.split_once(':')
                .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect();
    (status, headers, body.to_string())
}

/// The value of one header, or `""`.
fn header<'a>(headers: &'a [(String, String)], name: &str) -> &'a str {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .unwrap_or("")
}

#[test]
fn the_entry_document_is_served_at_the_root() {
    let root = built_frontend("root");
    let addr = serve(Some(root), Arc::new(NoAuth));

    let (status, headers, body) = get(addr, "/", None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(header(&headers, "content-type"), "text/html; charset=utf-8");
    // An `index.html` names hashed assets, so it must never be cached.
    assert_eq!(header(&headers, "cache-control"), "no-cache");
    assert!(body.contains("<div id=\"root\"></div>"), "{body}");
}

#[test]
fn a_hashed_asset_is_served_with_its_own_type() {
    let root = built_frontend("asset");
    let addr = serve(Some(root), Arc::new(NoAuth));

    let (status, headers, body) = get(addr, "/assets/index-abc123.js", None);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        header(&headers, "content-type"),
        "text/javascript; charset=utf-8"
    );
    // The hash is the content's name, so this one may be cached forever.
    assert_eq!(
        header(&headers, "cache-control"),
        "public, max-age=31536000, immutable"
    );
    assert!(body.contains("export const root = 1;"), "{body}");
}

#[test]
fn a_path_that_escapes_the_web_root_is_refused() {
    let root = built_frontend("escape");
    // A file the traversal would reach if the name were followed: the assertion
    // that matters is that its content never comes back.
    let decoy = root
        .parent()
        .expect("parent")
        .join("riscdom-webui-secret.txt");
    std::fs::write(&decoy, "TOP SECRET\n").expect("decoy");

    let addr = serve(Some(root), Arc::new(NoAuth));

    for path in [
        "/assets/../../riscdom-webui-secret.txt",
        "/assets/..%2f..%2friscdom-webui-secret.txt",
        "/../riscdom-webui-secret.txt",
    ] {
        let (status, _, body) = get(addr, path, None);
        assert_eq!(status, 404, "{path} answered {status}: {body}");
        assert!(!body.contains("TOP SECRET"), "{path} leaked: {body}");
    }
}

#[test]
fn a_missing_asset_is_a_404_that_names_it() {
    let root = built_frontend("missing");
    let addr = serve(Some(root), Arc::new(NoAuth));

    let (status, _, body) = get(addr, "/assets/index-gone.js", None);
    assert_eq!(status, 404, "{body}");
    assert!(body.contains("index-gone.js"), "{body}");
    assert!(body.contains("not_found"), "{body}");
}

#[test]
fn without_a_web_root_the_namespace_says_so() {
    let addr = serve(None, Arc::new(NoAuth));

    let (status, _, body) = get(addr, "/", None);
    assert_eq!(status, 404);
    // The message is the point: the operator learns how to turn the UI on.
    assert!(body.contains("--web-root"), "{body}");

    // A path outside the namespace is the route table's 404, unchanged.
    let (status, _, body) = get(addr, "/nothing-here", None);
    assert_eq!(status, 404);
    assert!(body.contains("not_found"), "{body}");
}

#[test]
fn the_web_ui_needs_no_token_while_the_api_still_does() {
    let root = built_frontend("auth");
    let addr = serve(Some(root), Arc::new(TokenAuth::new("secret-token")));

    // A document and a script carry no secret: they are served as they are.
    assert_eq!(get(addr, "/", None).0, 200);
    assert_eq!(get(addr, "/assets/index-abc123.js", None).0, 200);

    // Everything behind `/v0/` is still the token's business.
    assert_eq!(get(addr, "/v0/health", None).0, 401);
    let (status, _, body) = get(addr, "/v0/health", Some("secret-token"));
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("\"status\":\"ok\""), "{body}");

    // The unknown-API-path answer is the error model, not the Web UI.
    let (status, _, body) = get(addr, "/v0/nope", Some("secret-token"));
    assert_eq!(status, 404);
    assert!(body.contains("not_found"), "{body}");
}
