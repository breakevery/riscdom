//! How the server is configured: bind address, heartbeat, auth hook.

use crate::auth::{Authn, NoAuth};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// The address `riscdom-server` binds when nothing says otherwise. The loopback
/// default is deliberate: the open-source build ships plaintext HTTP, so it does
/// not listen on a public interface unless told to.
pub const DEFAULT_BIND: &str = "127.0.0.1:7821";

/// The heartbeat period (15 s), as documented in `docs/control-plane-events.md` §1.
pub const DEFAULT_HEARTBEAT_MS: u64 = 15_000;

/// Everything `Server::new` needs.
#[derive(Clone)]
pub struct ServerConfig {
    /// Where to listen.
    pub bind: SocketAddr,
    /// Heartbeat period, or `None` to disable it (tests do this).
    pub heartbeat: Option<Duration>,
    /// The authentication hook. Defaults to [`NoAuth`] (v0.9).
    pub authn: Arc<dyn Authn>,
}

impl ServerConfig {
    /// A config with the documented defaults, bound to `bind`.
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            bind,
            heartbeat: Some(Duration::from_millis(DEFAULT_HEARTBEAT_MS)),
            authn: Arc::new(NoAuth),
        }
    }

    /// Override the heartbeat period (`None` disables it).
    pub fn with_heartbeat(mut self, heartbeat: Option<Duration>) -> Self {
        self.heartbeat = heartbeat;
        self
    }

    /// Install a different authentication hook.
    pub fn with_authn(mut self, authn: Arc<dyn Authn>) -> Self {
        self.authn = authn;
        self
    }
}
