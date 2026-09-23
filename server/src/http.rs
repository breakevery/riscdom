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
}

impl Shared {
    /// One runtime line, if the operator asked for lines at this level.
    fn log(&self, level: LogLevel, message: impl std::fmt::Display) {
        log::line(self.log_level, level, message);
    }

    fn uptime_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
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
            if method == "POST" {
                match read_json_body(request).await {
                    Ok(Some(body)) => params.merge(body),
                    Ok(None) => {}
                    Err(response) => return *response,
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
                routes::dispatch(action, &params, &app, &hub, log_level, &actor)
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
