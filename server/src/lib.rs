//! The RiscDom control plane (v0.9): HTTP + SSE over the host's kernel facade.
//!
//! **Layer 3, the `server` host.** It sits beside `ui/src-tauri` and depends only
//! on Layer 2 (the `host` public API), never on `agent` / `sandbox` / `audit`
//! directly, and never on a Tauri type. `tauri` is still *linked* (because `host`
//! depends on it unconditionally) — a known cost, not a reference.
//!
//! The interface this implements is the one settled in
//! `docs/control-plane-api.md` and `docs/control-plane-events.md`. What is here:
//!
//! - the **26 query endpoints** and the **27 control endpoints** of the API
//!   tables (`routes.rs`), plus the reserved `/v0/resources` and
//!   `POST /v0/vm/start` (both `501`) and the host-local `/v0/health`,
//!   `/v0/status`;
//! - the **event stream** (`GET /v0/events`), carrying the envelope built in
//!   `host_core::events`, with a bounded replay buffer for `Last-Event-ID` and a `gap`
//!   frame when the hole is older than the buffer;
//! - the **error model**, the **`Authn` hook**, and the **bearer token** the
//!   served program installs by default ([`TokenAuth`], `--no-auth` to opt out).
//!
//! Deliberate gaps, marked where they live:
//!
//! - capability checks are *declared* per route and handed to the hook, but their
//!   enforcement is the permission intermediary's job (a later batch).

pub mod auth;
pub mod cli;
pub mod config;
pub mod envelope;
pub mod http;
pub mod io;
pub mod routes;
pub mod sse;
pub mod token;

pub use auth::{Actor, ActorKind, AuthError, Authn, Capability, NoAuth, ReqMeta, TokenAuth};
pub use cli::{AuthMode, Cli, DEFAULT_BIND, DEFAULT_HEARTBEAT_MS, USAGE};
pub use config::ServerConfig;
pub use envelope::{Frame, ENVELOPE_VERSION};
pub use http::{Running, Server};
pub use sse::{HttpEventSink, Replay, SseHub, WireFrame, REPLAY_CAPACITY};
pub use token::{TokenError, TokenFile, TOKEN_BYTES, TOKEN_FILE};

/// The crate version reported by `/v0/health` and `/v0/status`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
