//! The HTTP server: routing, the error model, the SSE stream (with replay), and
//! the authentication step every request goes through.
//!
//! The endpoint surface lives in [`crate::routes`]; this module owns the
//! connection loop, the request-to-params plumbing, and the response shapes.

use crate::auth::{Authn, ReqMeta};
use crate::config::ServerConfig;
use crate::io::TokioIo;
use crate::log::{self, LogLevel};
use crate::routes::{self, Local, Resolution};
use crate::sse::{HttpEventSink, Replay, SseHub, CHANNEL_CAPACITY};
use futures_util::stream;
use host_core::AppState;
use http_body_util::{combinators::BoxBody, BodyExt, Full, Limited, StreamBody};
use hyper::body::{Bytes, Frame};
use hyper::header::{HeaderValue, AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_TYPE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// Every response body: either a fixed buffer or the SSE stream, type-erased so
/// one response type covers both.
pub(crate) type RespBody = BoxBody<Bytes, std::io::Error>;

/// The largest request body the server will read. Control requests are small
/// JSON objects; anything bigger is a mistake or an attack, and either way it is
/// not read into memory.
pub(crate) const MAX_BODY_BYTES: usize = 64 * 1024;

/// The largest project archive an import will accept (v0.9 project in/out).
///
/// Its own ceiling on purpose: `MAX_BODY_BYTES` exists because a control body is a
/// small JSON object, and a project archive is neither small nor a control body.
/// Raising the shared limit to fit one import would let every other endpoint be
/// handed a megabyte, so the import carries its own and the shared one stays.
/// Public because the limit is part of the contract a client plans around (and
/// because the test that proves `413` is worth more when it uses the real number).
pub const MAX_IMPORT_BYTES: usize = 64 * 1024 * 1024;

/// A server, before it is bound.
pub struct Server {
    app: Arc<AppState>,
    cfg: ServerConfig,
    hub: Arc<SseHub>,
    started: Instant,
}

impl Server {
    /// Build the server over the host instance it fronts.
    ///
    /// The heartbeat thread starts here when a period is configured.
    pub fn new(app: Arc<AppState>, cfg: ServerConfig) -> Self {
        let hub = SseHub::new(CHANNEL_CAPACITY);
        if let Some(period) = cfg.heartbeat {
            hub.start_heartbeat(period, "keep-alive");
        }
        Self {
            app,
            cfg,
            hub,
            started: Instant::now(),
        }
    }

    /// The sink to hand to `AppState::run_agent` as its `emitter` argument.
    ///
    /// This is how the host's events reach the stream without `host-core` changing:
    /// the sink is a parameter of the run, not a field of the state.
    ///
    /// It takes no identity argument on purpose. Identity is a property of the
    /// **source** ([`AppState::agent_id`]), not of the transport, so every sink
    /// this process builds carries the same one — the rule stated in
    /// [`HttpEventSink::new`], enforced here by not offering the choice.
    pub fn sink(&self) -> HttpEventSink {
        HttpEventSink::new(Arc::clone(&self.hub), self.app.agent_id())
    }

    /// Live SSE subscribers right now.
    pub fn subscribers(&self) -> usize {
        self.hub.subscribers()
    }

    /// Bind and start accepting.
    pub async fn start(self) -> std::io::Result<Running> {
        let listener = TcpListener::bind(self.cfg.bind).await?;
        let local_addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            app: self.app,
            authn: Arc::clone(&self.cfg.authn),
            hub: Arc::clone(&self.hub),
            started: self.started,
            connections: Arc::new(AtomicUsize::new(0)),
            log_level: self.cfg.log_level,
            web_root: self.cfg.web_root.clone(),
        });
        let task = tokio::spawn(accept_loop(listener, shared));
        Ok(Running { local_addr, task })
    }
}

/// A bound, running server.
pub struct Running {
    local_addr: SocketAddr,
    task: JoinHandle<()>,
}

impl Running {
    /// The address actually bound (useful with port `0`).
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Stop accepting. In-flight connections end when their client disconnects.
    pub fn abort(&self) {
        self.task.abort();
    }
}

/// What every handler shares.
struct Shared {
    app: Arc<AppState>,
    authn: Arc<dyn Authn>,
    hub: Arc<SseHub>,
    started: Instant,
    connections: Arc<AtomicUsize>,
    /// How much the library logs (off unless the operator asked).
    log_level: LogLevel,
    /// The built Web UI this process serves, when it was given one (v0.9 D2a).
    web_root: Option<PathBuf>,
}

impl Shared {
    /// One runtime line, if the operator asked for lines at this level.
    fn log(&self, level: LogLevel, message: impl std::fmt::Display) {
        log::line(self.log_level, level, message);
    }

    fn uptime_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    /// The built Web UI, when `--web-root` names one (v0.9 D2a).
    ///
    /// In the namespace: `/` (the entry document) and `/assets/*` (its hashed
    /// files), for `GET` only. The frontend's own routes are not this server's
    /// business, so there is deliberately **no SPA fallback** — a path outside the
    /// namespace falls through to the route table and gets the same answer it got
    /// before, and the table grows no route for any of this.
    ///
    /// Nothing here is authenticated: an HTML document, a stylesheet and a script
    /// carry no secret. Everything behind `/v0/*` still passes through `Authn`.
    ///
    /// `None` means "not this namespace — keep routing".
    fn web_ui(&self, path: &str) -> Option<Response<RespBody>> {
        if path == "/" {
            return Some(self.web_asset(path, "index.html"));
        }
        if let Some(rest) = path.strip_prefix("/assets/") {
            if rest.is_empty() {
                return Some(error_response(
                    404,
                    "not_found",
                    "/assets/ names no file",
                    Some("path"),
                ));
            }
            return Some(self.web_asset(path, &format!("assets/{rest}")));
        }
        None
    }

    /// One file under the Web UI root, or the 404 that says why not.
    fn web_asset(&self, requested: &str, relative: &str) -> Response<RespBody> {
        let Some(root) = self.web_root.as_deref() else {
            return error_response(
                404,
                "not_found",
                &format!(
                    "{requested} belongs to the Web UI, and this server serves none: restart it \
                     with --web-root <dir> pointing at the built frontend"
                ),
                Some("path"),
            );
        };

        // The name the client sent is never percent-decoded, so an encoded `..` is
        // a name that does not exist rather than a traversal; a literal `..`, a root
        // or a prefix component is refused the way `workspace_io::safe_relative`
        // refuses it. This check is about the *name*.
        let nested = Path::new(relative);
        if nested.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return error_response(
                404,
                "not_found",
                &format!("{requested} is not a path inside the Web UI"),
                Some("path"),
            );
        }

        // …and the file that was actually found is checked too: a symbolic link
        // inside the root can point outside it, and reading would follow it. The
        // root is resolved per request because the root itself may be a link.
        let Ok(canonical_root) = std::fs::canonicalize(root) else {
            return self.no_such_asset(requested);
        };
        let Ok(canonical) = std::fs::canonicalize(root.join(nested)) else {
            return self.no_such_asset(requested);
        };
        if !canonical.starts_with(&canonical_root) {
            return error_response(
                404,
                "not_found",
                &format!("{requested} resolves outside the Web UI directory"),
                Some("path"),
            );
        }
        let Ok(bytes) = std::fs::read(&canonical) else {
            return self.no_such_asset(requested);
        };

        let mut response = binary_response(
            StatusCode::OK,
            content_type_for(&canonical),
            Bytes::from(bytes),
        );
        // A hashed asset names its own content, so it can be cached forever; the
        // entry document must not be, or a browser keeps an `index.html` that asks
        // for assets a later build deleted.
        let cache = if relative == "index.html" {
            "no-cache"
        } else {
            "public, max-age=31536000, immutable"
        };
        if let Ok(value) = HeaderValue::from_str(cache) {
            response.headers_mut().insert(CACHE_CONTROL, value);
        }
        response
    }

    /// The one 404 for "the Web UI is configured and this file is not in it".
    fn no_such_asset(&self, requested: &str) -> Response<RespBody> {
        error_response(
            404,
            "not_found",
            &format!("{requested} is not a file in the Web UI directory"),
            Some("path"),
        )
    }

    fn health(&self) -> Response<RespBody> {
        json_response(
            StatusCode::OK,
            &serde_json::json!({
                "status": "ok",
                "version": crate::VERSION,
                "uptime_ms": self.uptime_ms(),
            }),
        )
    }

    fn status(&self) -> Response<RespBody> {
        json_response(
            StatusCode::OK,
            &serde_json::json!({
                "status": "ok",
                "version": crate::VERSION,
                "uptime_ms": self.uptime_ms(),
                "connections": self.connections.load(Ordering::Relaxed),
                "sse_subscribers": self.hub.subscribers(),
                // This host instance is the one agent the control plane knows until
                // the executor roster is wired (later batches).
                "agents": 1,
                "agent_id": self.app.agent_id(),
            }),
        )
    }

    /// The endpoints this crate answers itself.
    fn local(&self, kind: Local, last_event_id: Option<&str>) -> Response<RespBody> {
        match kind {
            Local::Health => self.health(),
            Local::Status => self.status(),
            Local::Events => self.open_stream(last_event_id),
        }
    }

    /// Open an SSE stream: `hello`, then whatever a reconnecting client is owed,
    /// then live frames.
    ///
    /// The subscription is taken **before** the replay is computed, so a frame
    /// published during the handover is delivered live and then skipped by the
    /// ordinal check in the body — never lost, never sent twice.
    fn open_stream(&self, last_event_id: Option<&str>) -> Response<RespBody> {
        let rx = self.hub.subscribe();
        let mut pending: VecDeque<Bytes> = VecDeque::new();
        pending.push_back(Bytes::from(
            self.hub.hello(self.app.agent_id()).bytes.clone(),
        ));

        let mut last_seq = 0u64;
        match self.hub.replay_after(last_event_id) {
            Replay::Nothing => {}
            Replay::Frames(frames) => {
                for frame in frames {
                    last_seq = last_seq.max(frame.seq);
                    pending.push_back(Bytes::from(frame.bytes.clone()));
                }
            }
            Replay::Gap { lost_after, frames } => {
                pending.push_back(Bytes::from(self.hub.gap(&lost_after).bytes.clone()));
                for frame in frames {
                    last_seq = last_seq.max(frame.seq);
                    pending.push_back(Bytes::from(frame.bytes.clone()));
                }
            }
        }

        type Item = Result<Frame<Bytes>, std::io::Error>;
        type Seed = (
            broadcast::Receiver<Arc<crate::sse::WireFrame>>,
            VecDeque<Bytes>,
            u64,
        );
        let frames = stream::unfold(
            (rx, pending, last_seq) as Seed,
            |(mut rx, mut pending, last_seq)| async move {
                if let Some(bytes) = pending.pop_front() {
                    let item: Item = Ok(Frame::data(bytes));
                    return Some((item, (rx, pending, last_seq)));
                }
                loop {
                    match rx.recv().await {
                        Ok(frame) => {
                            // A frame the replay already covered, or one older
                            // than the cursor: the client has it.
                            if frame.id.is_some() && frame.seq <= last_seq {
                                continue;
                            }
                            let item: Item = Ok(Frame::data(Bytes::from(frame.bytes.clone())));
                            return Some((item, (rx, pending, last_seq)));
                        }
                        // A subscriber that fell behind loses what it missed; the
                        // `Last-Event-ID` handshake is how a client repairs that.
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return None,
                    }
                }
            },
        );

        let mut response = Response::new(StreamBody::new(frames).boxed());
        *response.status_mut() = StatusCode::OK;
        let headers = response.headers_mut();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive"));
        response
    }
}

/// Counts a connection for as long as it is open.
struct ConnGuard(Arc<AtomicUsize>);

impl Drop for ConnGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

async fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            Err(e) => {
                shared.log(LogLevel::Error, format!("accept failed: {e}"));
                continue;
            }
        };
        shared.connections.fetch_add(1, Ordering::Relaxed);
        let guard = ConnGuard(Arc::clone(&shared.connections));
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let _guard = guard;
            if let Err(e) = serve(stream, shared.clone()).await {
                // The peer and the error only: a request's credential never gets here.
                shared.log(LogLevel::Info, format!("connection from {peer} ended: {e}"));
            }
        });
    }
}

async fn serve(stream: TcpStream, shared: Arc<Shared>) -> Result<(), hyper::Error> {
    let io = TokioIo::new(stream);
    let service = service_fn(move |req| {
        let shared = Arc::clone(&shared);
        async move { Ok::<_, Infallible>(handle(req, shared).await) }
    });
    http1::Builder::new().serve_connection(io, service).await
}

/// Authenticate, then route. Auth runs first and for every request.
async fn handle(
    request: Request<hyper::body::Incoming>,
    shared: Arc<Shared>,
) -> Response<RespBody> {
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(str::to_string);
    let presented = presented_credential(request.headers().get(AUTHORIZATION));
    let last_event_id = request
        .headers()
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    // The built Web UI comes **before** the route table and before `Authn` (v0.9
    // D2a): the assets carry no secret, and everything behind `/v0/*` still
    // authenticates below. `None` means "not this namespace", so routing continues
    // exactly as it did.
    if method == "GET" {
        if let Some(response) = shared.web_ui(&path) {
            return response;
        }
    }

    let resolution = routes::resolve(&method, &path);
    let capability = match &resolution {
        Resolution::Query { capability, .. } | Resolution::Local { capability, .. } => {
            Some(*capability)
        }
        _ => None,
    };
    let meta = ReqMeta {
        method: method.clone(),
        path: path.clone(),
        token: presented,
        capability,
    };
    let actor = match shared.authn.authorise(&meta) {
        Ok(actor) => actor,
        Err(err) => return error_response(err.status(), err.code(), err.message(), None),
    };
    // Default deny: whatever capability the route declares has to be one this
    // actor holds. The route table cannot express "none", so every served path
    // is checked; a 404/405 has no capability and is answered below.
    if let Some(capability) = capability {
        if !actor.allows(capability) {
            return error_response(
                403,
                "forbidden",
                &format!("the actor may not {}", capability.as_str()),
                Some("capability"),
            );
        }
    }

    match resolution {
        Resolution::Local { kind, .. } => shared.local(kind, last_event_id.as_deref()),
        Resolution::MethodNotAllowed { allowed } => error_response(
            405,
            "method_not_allowed",
            &format!("{method} is not allowed on {path}; use {allowed}"),
            Some("method"),
        ),
        Resolution::NotFound => error_response(
            404,
            "not_found",
            &format!("no endpoint {method} {path}"),
            None,
        ),
        Resolution::Query {
            action, path_param, ..
        } => {
            let mut params = routes::parse_query(query.as_deref());
            if let Some((name, value)) = path_param {
                params.insert(name, value);
            }
            // One endpoint takes bytes instead of a JSON object (v0.9 project
            // in/out). The JSON path is unchanged and still the default: this is a
            // branch on what the route's body *is*, not a change to how bodies are
            // read.
            let mut archive: Option<Bytes> = None;
            if method == "POST" {
                if action == routes::Action::WorkspaceImport {
                    let content_type = request
                        .headers()
                        .get(CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_string();
                    params.insert("content_type", content_type);
                    match read_binary_body(request, MAX_IMPORT_BYTES, "archive").await {
                        Ok(bytes) => archive = Some(bytes),
                        Err(response) => return *response,
                    }
                } else {
                    match read_json_body(request).await {
                        Ok(Some(body)) => params.merge(body),
                        Ok(None) => {}
                        Err(response) => return *response,
                    }
                }
            }
            let app = Arc::clone(&shared.app);
            let hub = Arc::clone(&shared.hub);
            let log_level = shared.log_level;
            let actor = actor.clone();
            // The host's work is synchronous, and some of it is heavy (a compile,
            // a `--version` probe, a download): it runs off the async runtime, so
            // a request never stalls the event stream.
            match tokio::task::spawn_blocking(move || {
                routes::dispatch(
                    action,
                    &params,
                    &app,
                    &hub,
                    log_level,
                    &actor,
                    archive.as_deref(),
                )
            })
            .await
            {
                Ok(response) => response,
                Err(_) => error_response(500, "internal", "the request task failed", None),
            }
        }
    }
}

/// Read a JSON object body into parameters.
///
/// `Ok(None)` means "no body at all", which is fine for the endpoints whose
/// parameters are all optional.
async fn read_json_body(
    request: Request<hyper::body::Incoming>,
) -> Result<Option<routes::Params>, Box<Response<RespBody>>> {
    let limited = Limited::new(request.into_body(), MAX_BODY_BYTES);
    let collected = match limited.collect().await {
        Ok(collected) => collected,
        Err(_) => {
            return Err(Box::new(error_response(
                400,
                "bad_request",
                "the request body could not be read (or is larger than 64 KiB)",
                Some("body"),
            )))
        }
    };
    let bytes = collected.to_bytes();
    if bytes.is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(e) => {
            return Err(Box::new(error_response(
                400,
                "bad_request",
                &format!("the request body is not JSON: {e}"),
                Some("body"),
            )))
        }
    };
    if !value.is_object() {
        return Err(Box::new(error_response(
            400,
            "bad_request",
            "the request body must be a JSON object",
            Some("body"),
        )));
    }
    Ok(Some(routes::Params::from_json(&value)))
}

/// Read a **binary** request body, up to `limit` bytes (v0.9 project in/out).
///
/// The sibling of [`read_json_body`] for the one endpoint whose body is not a JSON
/// object. It differs in exactly two ways: the ceiling is the caller's (an import
/// says how big an archive it accepts), and the bytes are handed back rather than
/// parsed. Over the limit is `413`, not the JSON path's `400`, because the caller
/// sent something well-formed and simply too large — a distinction a client can
/// act on (raise the limit, split the archive) where `400` would only say "bad".
async fn read_binary_body(
    request: Request<hyper::body::Incoming>,
    limit: usize,
    what: &str,
) -> Result<Bytes, Box<Response<RespBody>>> {
    let limited = Limited::new(request.into_body(), limit);
    match limited.collect().await {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(_) => Err(Box::new(error_response(
            413,
            "payload_too_large",
            &format!(
                "the {what} is larger than the {} bytes this host accepts",
                limit
            ),
            Some("body"),
        ))),
    }
}

/// The `Content-Type` for a file the Web UI serves (v0.9 D2a).
///
/// A short explicit map rather than a MIME crate: the built frontend's file set is
/// known (`index.html` plus the hashed `.js` / `.css`), the extra entries cover what
/// a Vite/Tauri build may drop beside them, and the charset matters on the text
/// types. An unknown extension is `application/octet-stream`, which makes a browser
/// download the file instead of guessing at it.
fn content_type_for(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// A `200`-family response whose body is bytes, not JSON (v0.9 project in/out).
///
/// The first non-JSON body on this surface apart from the event stream. The type
/// does not change — [`RespBody`] has always been `BoxBody<Bytes, _>` — so this is
/// a helper, not a new shape: the caller says what the bytes are.
pub(crate) fn binary_response(
    status: StatusCode,
    content_type: &str,
    bytes: Bytes,
) -> Response<RespBody> {
    let mut response = Response::new(full_body(bytes));
    *response.status_mut() = status;
    if let Ok(value) = HeaderValue::from_str(content_type) {
        response.headers_mut().insert(CONTENT_TYPE, value);
    }
    response
}

/// The credential an `Authorization` header presents, when it is one.
fn presented_credential(value: Option<&HeaderValue>) -> Option<String> {
    let raw = value?.to_str().ok()?;
    let (scheme, credential) = raw.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !credential.is_empty() {
        Some(credential.to_string())
    } else {
        None
    }
}

/// A body that is already complete in memory.
fn full_body(bytes: Bytes) -> RespBody {
    Full::new(bytes)
        .map_err(|never: Infallible| match never {})
        .boxed()
}

pub(crate) fn json_response(status: StatusCode, value: &serde_json::Value) -> Response<RespBody> {
    let bytes = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec());
    let mut response = Response::new(full_body(Bytes::from(bytes)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

/// The answer to an action that made no content.
pub(crate) fn no_content() -> Response<RespBody> {
    let mut response = Response::new(full_body(Bytes::new()));
    *response.status_mut() = StatusCode::NO_CONTENT;
    response
}

/// The error model of `docs/control-plane-api.md` §4.
pub(crate) fn error_response(
    status: u16,
    code: &str,
    message: &str,
    cause: Option<&str>,
) -> Response<RespBody> {
    let body = serde_json::json!({
        "code": code,
        "message": message,
        "retryable": false,
        "cause": cause,
    });
    json_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        &body,
    )
}
