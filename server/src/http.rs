//! The HTTP server: routing, the error model, and the SSE response body.
//!
//! Two smoke endpoints and the event stream — the rest of the API table in
//! `docs/control-plane-api.md` is later batches.

use crate::auth::{Authn, ReqMeta};
use crate::config::ServerConfig;
use crate::envelope::{now_ms, Envelope};
use crate::io::TokioIo;
use crate::sse::{frame_bytes, HttpEventSink, SseFrame, SseHub, CHANNEL_CAPACITY};
use futures_util::stream;
use host::AppState;
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use hyper::header::{HeaderValue, AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_TYPE};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
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
    /// This is how the host's events reach the stream without `host` changing:
    /// the sink is a parameter of the run, not a field of the state.
    pub fn sink(&self, agent_id: &str) -> HttpEventSink {
        HttpEventSink::new(Arc::clone(&self.hub), agent_id)
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
}

impl Shared {
    fn uptime_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn health(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "ok",
            "version": crate::VERSION,
            "uptime_ms": self.uptime_ms(),
        })
    }

    fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "ok",
            "version": crate::VERSION,
            "uptime_ms": self.uptime_ms(),
            "connections": self.connections.load(Ordering::Relaxed),
            "sse_subscribers": self.hub.subscribers(),
            // This host instance is the one agent the control plane knows until
            // the executor roster is wired (later batches).
            "agents": 1,
            "agent_id": self.app.agent_id(),
        })
    }

    /// Open an SSE stream: subscribe first, then the `hello` frame.
    ///
    /// Subscribing happens here rather than inside the body, so a publish that
    /// follows the client having *read* `hello` can never race the subscription.
    fn open_stream(&self) -> Response<RespBody> {
        let rx = self.hub.subscribe();
        let hello = Envelope::hello(
            self.app.agent_id().to_string(),
            now_ms(),
            serde_json::json!({ "from": 0, "to": 0 }),
            serde_json::json!({ "event": [], "agent_id": null, "task_id": null }),
        );
        let first = Bytes::from(frame_bytes(&SseFrame::Envelope(hello), 0));

        type Item = Result<Frame<Bytes>, std::io::Error>;
        type Seed = (broadcast::Receiver<SseFrame>, u64, Option<Bytes>);
        let frames = stream::unfold(
            (rx, 0u64, Some(first)) as Seed,
            |(mut rx, mut seq, mut pending)| async move {
                if let Some(bytes) = pending.take() {
                    let item: Item = Ok(Frame::data(bytes));
                    return Some((item, (rx, seq, pending)));
                }
                loop {
                    match rx.recv().await {
                        Ok(frame) => {
                            seq += 1;
                            let bytes = Bytes::from(frame_bytes(&frame, seq));
                            let item: Item = Ok(Frame::data(bytes));
                            return Some((item, (rx, seq, pending)));
                        }
                        // A subscriber that fell behind loses what it missed.
                        // Filling the hole is the `gap` frame's job and `gap` is
                        // **not implemented in this batch**
                        // (`docs/control-plane-events.md` §2), so here the stream
                        // simply carries on.
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
                eprintln!("riscdom-server: accept failed: {e}");
                continue;
            }
        };
        shared.connections.fetch_add(1, Ordering::Relaxed);
        let guard = ConnGuard(Arc::clone(&shared.connections));
        let shared = Arc::clone(&shared);
        tokio::spawn(async move {
            let _guard = guard;
            if let Err(e) = serve(stream, shared).await {
                // The peer and the error only: a request's token never reaches here.
                eprintln!("riscdom-server: connection from {peer} ended: {e}");
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
    let meta = ReqMeta {
        method: method.clone(),
        path: path.clone(),
        token: bearer(request.headers().get(AUTHORIZATION)),
    };
    if let Err(err) = shared.authn.authorise(&meta) {
        return error_response(err.status(), err.code(), err.message(), None);
    }
    match (method.as_str(), path.as_str()) {
        ("GET", "/v0/health") => json_response(StatusCode::OK, &shared.health()),
        ("GET", "/v0/status") => json_response(StatusCode::OK, &shared.status()),
        ("GET", "/v0/events") => shared.open_stream(),
        // Every other (method, path) pair, including a known path under the wrong
        // method: the error model has no `method_not_allowed`, and inventing a
        // code would break its closed list.
        _ => error_response(
            404,
            "not_found",
            &format!("no endpoint {method} {path}"),
            None,
        ),
    }
}

/// The token from an `Authorization: Bearer <token>` header, if it is one.
fn bearer(value: Option<&HeaderValue>) -> Option<String> {
    let raw = value?.to_str().ok()?;
    let (scheme, token) = raw.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        Some(token.to_string())
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

fn json_response(status: StatusCode, value: &serde_json::Value) -> Response<RespBody> {
    let bytes = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec());
    let mut response = Response::new(full_body(Bytes::from(bytes)));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

/// The error model of `docs/control-plane-api.md` §4.
fn error_response(
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
