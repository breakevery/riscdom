//! Platform abstraction for QEMU endpoints.
//!
//! All platform-specific transport decisions live here. Callers must never
//! hardcode `unix:` sockets or separated paths; they build an endpoint and
//! let [`QmpEndpoint::to_qemu_arg`] / [`SerialEndpoint::to_qemu_arg`] render
//! the QEMU argument.
//!
//! Platform policy:
//! - **Windows**: use TCP (`127.0.0.1:<port>`). Unix sockets are unavailable.
//! - **Unix**: prefer Unix sockets for QMP.
//!
//! Testing (v0.7 batch A): the Unix arm's *rendering* is a pure function
//! ([`unix_qmp_arg`]) and is unit-tested on every platform, Windows included. What
//! is **not** exercised here is a real Unix socket end to end 鈥?that needs macOS or
//! Linux and a QEMU build, and is listed as an untested path in `docs/handoff.md`.
//! The TCP path, meanwhile, is what the suite drives on Windows.

use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

/// QMP (QEMU Machine Protocol) endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QmpEndpoint {
    Tcp {
        host: String,
        port: u16,
    },
    #[cfg(unix)]
    UnixSocket {
        path: PathBuf,
    },
}

/// Serial (UART) endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SerialEndpoint {
    Tcp { host: String, port: u16 },
    File { path: PathBuf },
}

impl QmpEndpoint {
    /// TCP QMP endpoint. This is the default on Windows.
    pub fn tcp(host: impl Into<String>, port: u16) -> Self {
        QmpEndpoint::Tcp {
            host: host.into(),
            port,
        }
    }

    /// Default endpoint for the current platform.
    /// On Unix this returns a Unix socket; on Windows a TCP endpoint.
    #[cfg(unix)]
    pub fn platform_default(path: impl Into<PathBuf>) -> Self {
        QmpEndpoint::UnixSocket { path: path.into() }
    }

    /// Default endpoint for the current platform (Windows: TCP).
    #[cfg(windows)]
    pub fn platform_default(port: u16) -> Self {
        QmpEndpoint::Tcp {
            host: "127.0.0.1".to_string(),
            port,
        }
    }

    /// Render as a QEMU `-qmp` argument value.
    ///
    /// QMP uses `wait=off` so that QEMU does not block on the monitor socket;
    /// the serial chardev (when waiting) is what gates guest start-up.
    pub fn to_qemu_arg(&self) -> String {
        match self {
            QmpEndpoint::Tcp { host, port } => {
                format!("tcp:{host}:{port},server=on,wait=off")
            }
            #[cfg(unix)]
            QmpEndpoint::UnixSocket { path } => unix_qmp_arg(path),
        }
    }
}

/// The `-qmp` value for a Unix socket (`unix:<path>,server=on,wait=off`).
///
/// Pure, so it is unit-tested on every platform 鈥?including Windows, which cannot
/// bind the socket but can still check the argument QEMU would be handed.
/// `wait=off` for the same reason as the TCP arm above.
pub fn unix_qmp_arg(path: &Path) -> String {
    format!("unix:{},server=on,wait=off", path.display())
}

impl SerialEndpoint {
    /// TCP serial endpoint. This is the default on Windows.
    pub fn tcp(host: impl Into<String>, port: u16) -> Self {
        SerialEndpoint::Tcp {
            host: host.into(),
            port,
        }
    }

    /// File-backed serial endpoint (output is appended to a file).
    pub fn file(path: impl Into<PathBuf>) -> Self {
        SerialEndpoint::File { path: path.into() }
    }

    /// Render as a QEMU `-serial` argument value.
    ///
    /// TCP serial uses `wait=on`: QEMU blocks until the host connects, which
    /// guarantees no early guest output is lost before the capture thread is
    /// attached.
    pub fn to_qemu_arg(&self) -> String {
        match self {
            SerialEndpoint::Tcp { host, port } => {
                format!("tcp:{host}:{port},server=on,wait=on")
            }
            SerialEndpoint::File { path } => format!("file:{}", path.display()),
        }
    }
}

/// Connect a TCP stream, retrying until `timeout` elapses.
///
/// Shared by the serial attach and the QMP client so both speak to QEMU the
/// same way and the retry logic lives in a single place.
pub(crate) fn connect_with_retry(
    addr: &str,
    timeout: std::time::Duration,
) -> Result<TcpStream, crate::error::SandboxError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match TcpStream::connect(addr) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                if std::time::Instant::now() >= deadline {
                    return Err(crate::error::SandboxError::Timeout(format!(
                        "connecting to {addr}: {e}"
                    )));
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qmp_tcp_arg_shape() {
        let e = QmpEndpoint::tcp("127.0.0.1", 4444);
        assert_eq!(e.to_qemu_arg(), "tcp:127.0.0.1:4444,server=on,wait=off");
    }

    #[test]
    fn serial_tcp_arg_shape() {
        let e = SerialEndpoint::tcp("127.0.0.1", 5555);
        assert_eq!(e.to_qemu_arg(), "tcp:127.0.0.1:5555,server=on,wait=on");
    }

    #[test]
    fn serial_file_arg_shape() {
        let e = SerialEndpoint::file(PathBuf::from("/tmp/serial.log"));
        assert!(e.to_qemu_arg().starts_with("file:"));
    }

    /// The Unix QMP argument is pinned on every platform; only the socket itself is
    /// Unix-only.
    #[test]
    fn unix_qmp_arg_shape() {
        assert_eq!(
            unix_qmp_arg(Path::new("/tmp/qmp.sock")),
            "unix:/tmp/qmp.sock,server=on,wait=off"
        );
    }

    /// The Unix endpoint renders exactly what the helper produces, so the two cannot
    /// drift apart on a Unix host.
    #[cfg(unix)]
    #[test]
    fn the_unix_socket_endpoint_uses_the_helper() {
        let path = PathBuf::from("/tmp/qmp.sock");
        let endpoint = QmpEndpoint::UnixSocket { path: path.clone() };
        assert_eq!(endpoint.to_qemu_arg(), unix_qmp_arg(&path));
    }
}
