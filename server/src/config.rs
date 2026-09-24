//! How the server is configured: bind address, heartbeat, auth hook, logging.

use crate::auth::{Authn, NoAuth};
use crate::cli::DEFAULT_HEARTBEAT_MS;
use crate::log::LogLevel;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Everything `Server::new` needs.
///
/// The **served program** always installs [`crate::TokenAuth`] (unless
/// `--no-auth`); this constructor's [`NoAuth`] default is for a library caller
/// that wires its own hook, which is what the tests do.
#[derive(Clone)]
pub struct ServerConfig {
    /// Where to listen.
    pub bind: SocketAddr,
    /// Heartbeat period, or `None` to disable it (tests do this).
    pub heartbeat: Option<Duration>,
    /// The authentication hook. Defaults to [`NoAuth`] (v0.9).
    pub authn: Arc<dyn Authn>,
    /// How much the library logs to stderr. Defaults to [`LogLevel::Off`], because
    /// the server is also run inside the CLI, whose stderr is its own.
    pub log_level: LogLevel,
    /// The built Web UI to serve at `/` and `/assets/*`, when there is one
    /// (v0.9 D2a). `None` — the default — serves the API only, exactly as before.
    pub web_root: Option<PathBuf>,
}

impl ServerConfig {
    /// A config with the documented defaults, bound to `bind`.
    pub fn new(bind: SocketAddr) -> Self {
        Self {
            bind,
            heartbeat: Some(Duration::from_millis(DEFAULT_HEARTBEAT_MS)),
            authn: Arc::new(NoAuth),
            log_level: LogLevel::Off,
            web_root: None,
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

    /// How much of the library's runtime logging to write to stderr.
    pub fn with_log_level(mut self, log_level: LogLevel) -> Self {
        self.log_level = log_level;
        self
    }

    /// Serve the built Web UI found in `dir` (v0.9 D2a).
    ///
    /// The directory is read at request time, not embedded in the binary: the
    /// frontend is a build artifact of a different toolchain (`npm run build`),
    /// and freezing it here would make every UI change a Rust rebuild.
    pub fn with_web_root(mut self, dir: impl Into<PathBuf>) -> Self {
        self.web_root = Some(dir.into());
        self
    }
}
