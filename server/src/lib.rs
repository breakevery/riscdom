//! The RiscDom control plane (v0.9): HTTP + SSE over the host's kernel facade.
//!
//! **Layer 3, the `server` host.** It sits beside `ui/src-tauri` and depends only
//! on Layer 2 (the `host` public API), never on `agent` / `sandbox` / `audit`
//! directly, and never on a Tauri type. `tauri` is still *linked* (because `host`
//! depends on it unconditionally) — a known cost, not a reference.
//!
//! The interface this implements is the one settled in
//! `docs/control-plane-api.md` and `docs/control-plane-events.md`. What is here
//! now:
//!
//! - the **26 query endpoints** of the API table (`routes.rs`), plus the reserved
//!   `/v0/resources` (501) and the host-local `/v0/health`, `/v0/status`;
//! - the **event stream** (`GET /v0/events`), carrying the envelope built in
//!   `host::events`;
//! - the **error model** and the **`Authn` hook** (`NoAuth` is the v0.9 default).
//!
//! Deliberate gaps, marked where they live:
//!
//! - control endpoints (`POST`, the API table's §5.2) are a later batch;
//! - `gap` frames and `Last-Event-ID` replay are a later batch;
//! - capability checks are *declared* per route and handed to the hook, but their
//!   enforcement is the permission intermediary's job (a later batch).

pub mod auth;
pub mod config;
pub mod envelope;
pub mod http;
pub mod io;
pub mod routes;
pub mod sse;

pub use auth::{Actor, ActorKind, AuthError, Authn, NoAuth, ReqMeta};
pub use config::{ServerConfig, DEFAULT_BIND, DEFAULT_HEARTBEAT_MS};
pub use envelope::Frame;
pub use http::{Running, Server};
pub use sse::{HttpEventSink, SseFrame, SseHub};

/// The crate version reported by `/v0/health` and `/v0/status`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
