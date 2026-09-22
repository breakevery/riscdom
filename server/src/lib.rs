//! The RiscDom control plane (v0.9): HTTP + SSE over the host's kernel facade.
//!
//! **Layer 3, the `server` host.** It sits beside `ui/src-tauri` and depends only
//! on Layer 2 (the `host` public API), never on `agent` / `sandbox` / `audit`
//! directly, and never on a Tauri type. `tauri` is still *linked* (because `host`
//! depends on it unconditionally) — a known cost, not a reference.
//!
//! The interface this implements is the one settled in
//! `docs/control-plane-api.md` and `docs/control-plane-events.md`. This crate is
//! the **skeleton**: two smoke endpoints (`/v0/health`, `/v0/status`), the SSE
//! stream (`/v0/events`), and the `Authn` hook. The remaining API endpoints are
//! later batches; nothing here changes `host` or its event emit sites.
//!
//! Deliberate gaps in this batch, marked in the code where they live:
//!
//! - `gap` frames and `Last-Event-ID` replay are **not implemented** (later batch);
//! - the emitted payloads are wrapped as-is, not normalised to the v1 shapes yet.

pub mod auth;
pub mod config;
pub mod envelope;
pub mod http;
pub mod io;
pub mod sse;

pub use auth::{Actor, ActorKind, AuthError, Authn, NoAuth, ReqMeta};
pub use config::{ServerConfig, DEFAULT_BIND, DEFAULT_HEARTBEAT_MS};
pub use envelope::{Envelope, ENVELOPE_VERSION};
pub use http::{Running, Server};
pub use sse::{HttpEventSink, SseFrame, SseHub};

/// The crate version reported by `/v0/health` and `/v0/status`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
