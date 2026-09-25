//! The LAN face: this desktop serving its own board (v0.9.9 内网接入 batch 3).
//!
//! The node the shell runs is the node a phone should be able to look at, so the
//! embedded server is started **over the app's own `Arc<AppState>`** — the same
//! instance, not a copy (a copy would have its own VM slot and its own settings,
//! and a board that can start a second QEMU is worse than no board at all).
//!
//! Three rules this module keeps:
//!
//! - **the settings decide, this acts**: nothing here reads the settings on its
//!   own; `apply` is handed them, by `setup` (a node left serving the LAN keeps
//!   serving it after a restart) and by the `set_network` command;
//! - **loopback unless asked otherwise**: `lan_allow_lan` is the switch that
//!   reaches a phone, and it is the only thing that binds `0.0.0.0`;
//! - **the server's life is the app's**: the `Running` handle is managed by the
//!   app and aborted on exit, so no socket outlives the window it was opened for.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use server::{Server, ServerConfig, TokenAuth};
use tauri::{AppHandle, Manager};

use host_tauri::settings::NetworkSettings;
use host_tauri::AppState;

/// The port the board is served on when the settings name none — the same default
/// `riscdom-server` uses.
pub const DEFAULT_LAN_PORT: u16 = 7821;

/// The running server, or `None`, plus why the last attempt did not start one.
/// Managed by the app, so it lives exactly as long as the app does.
#[derive(Default)]
pub struct LanServer {
    running: Mutex<Option<server::Running>>,
    problem: Mutex<Option<String>>,
}

impl LanServer {
    /// Is a board being served right now?
    pub fn is_running(&self) -> bool {
        self.running.lock().map(|held| held.is_some()).unwrap_or(false)
    }

    /// The address it is listening on, when one is running.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.running
            .lock()
            .ok()
            .and_then(|held| held.as_ref().map(|running| running.local_addr()))
    }

    /// Stop it. Idempotent: stopping what is not running is not an error.
    pub fn stop(&self) {
        if let Ok(mut held) = self.running.lock() {
            if let Some(running) = held.take() {
                running.abort();
            }
        }
    }

    /// Why the last start attempt failed (cleared by a successful one).
    pub fn problem(&self) -> Option<String> {
        self.problem.lock().ok().and_then(|held| held.clone())
    }

    fn set_problem(&self, next: Option<String>) {
        if let Ok(mut held) = self.problem.lock() {
            *held = next;
        }
    }
}

/// What the network page shows about the board (v0.9.9).
#[derive(Debug, Clone, serde::Serialize)]
pub struct LanStatus {
    /// Is the embedded server running?
    pub running: bool,
    /// The address it bound, when it is running (`0.0.0.0:7821` for a LAN board).
    pub bound: Option<String>,
    /// This machine's own address on the network, when one could be worked out.
    /// It is what a phone has to type, and it is deliberately *not* remembered
    /// anywhere: it changes with the network the machine is on.
    pub address: Option<String>,
    /// Why it is not running, when the settings asked for it and it would not
    /// start (a port already in use is the usual reason).
    pub problem: Option<String>,
}

/// The port the settings ask for: the port from `lan_bind`, or the default.
fn port_of(raw: &str) -> Option<u16> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(addr) = raw.parse::<SocketAddr>() {
        return Some(addr.port());
    }
    if let Ok(port) = raw.parse::<u16>() {
        return Some(port);
    }
    raw.rsplit(':').next()?.parse::<u16>().ok()
}

/// The address the settings ask for.
///
/// The **port** comes from `lan_bind` and the **host** from `lan_allow_lan`: the
/// switch that reaches a phone is the one thing that decides whether this listens
/// beyond loopback, so a typed `0.0.0.0` cannot quietly open a node whose switch
/// is off.
pub fn bind_for(settings: &NetworkSettings) -> SocketAddr {
    let port = settings
        .lan_bind
        .as_deref()
        .and_then(port_of)
        .unwrap_or(DEFAULT_LAN_PORT);
    let host = if settings.lan_allow_lan {
        [0, 0, 0, 0]
    } else {
        [127, 0, 0, 1]
    };
    SocketAddr::from((host, port))
}

/// This machine's address on the network, if it has one.
///
/// The socket is never used to send anything: `connect` on a UDP socket only picks
/// the route, so this works with no network, no dependency and no traffic.
fn lan_address() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.168.1.1:80").ok()?;
    let addr = socket.local_addr().ok()?;
    let ip = addr.ip();
    if ip.is_loopback() {
        return None;
    }
    Some(ip.to_string())
}

/// Where the built front end lives.
///
/// The bundled resource comes first and the source tree second, so a packaged app
/// and `tauri dev` resolve the same way without the caller knowing which it is.
pub fn resolve_web_root(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(dir) = app.path().resource_dir() {
        let bundled = dir.join("dist");
        if bundled.join("index.html").is_file() {
            return Some(bundled);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("dist");
    if dev.join("index.html").is_file() {
        return Some(dev);
    }
    None
}

/// Stop whatever is running, then start what the settings ask for. Idempotent.
///
/// A change to *any* setting means rebinding, which is why this always stops
/// first: a board that has moved from `127.0.0.1:7821` to `0.0.0.0:7821` is a
/// different socket, not a setting on the old one.
pub fn apply(app: &AppHandle, settings: &NetworkSettings) -> Result<(), String> {
    let lan = app.state::<LanServer>();
    lan.stop();
    if !settings.lan_enabled {
        return Ok(());
    }

    let state = Arc::clone(app.state::<Arc<AppState>>().inner());
    let token_file =
        server::token::load_or_create(state.data_dir()).map_err(|e| e.to_string())?;
    let addr = bind_for(settings);
    let mut config = ServerConfig::new(addr).with_authn(Arc::new(TokenAuth::new(token_file.token())));
    if let Some(root) = resolve_web_root(app) {
        config = config.with_web_root(root);
    }

    // The bind happens on Tauri's own tokio runtime, and the answer comes back
    // through a channel so a port already in use is reported to whoever asked for
    // the change instead of disappearing into a log line nobody reads.
    let (tx, rx) = std::sync::mpsc::channel();
    tauri::async_runtime::spawn(async move {
        let _ = tx.send(Server::new(state, config).start().await);
    });
    match rx.recv() {
        Ok(Ok(running)) => {
            let bound = running.local_addr();
            lan.running
                .lock()
                .map(|mut held| *held = Some(running))
                .map_err(|_| "the LAN server slot is poisoned".to_string())?;
            lan.set_problem(None);
            println!("riscdom: the node's board is served on http://{bound}");
            Ok(())
        }
        Ok(Err(e)) => {
            let message = format!("cannot bind {addr}: {e}");
            lan.set_problem(Some(message.clone()));
            Err(message)
        }
        Err(_) => {
            let message = "the LAN server task ended before it answered".to_string();
            lan.set_problem(Some(message.clone()));
            Err(message)
        }
    }
}

/// The board's state, for the network page (v0.9.9).
///
/// `problem` is only filled while the settings say the board should be up: a node
/// that is deliberately not serving is not a node with a problem.
pub fn status(app: &AppHandle, settings: Option<&NetworkSettings>) -> LanStatus {
    let lan = app.state::<LanServer>();
    let running = lan.is_running();
    let want = settings.map(|s| s.lan_enabled).unwrap_or(false);
    LanStatus {
        running,
        bound: lan.local_addr().map(|addr| addr.to_string()),
        address: if running { lan_address() } else { None },
        problem: if want && !running { lan.problem() } else { None },
    }
}
