//! Platform abstraction for QEMU endpoints.
//!
//! All platform-specific transport decisions live here. Callers must never
//! hardcode `unix:` sockets or separated paths; they build an endpoint and
//! let [`QmpEndpoint::to_qemu_arg`] / [`SerialEndpoint::to_qemu_arg`] render
//! the QEMU argument.
//!
//! Platform policy:
//! - **Windows**: use TCP (`127.0.0.1:<port>`). Unix sockets are unavailable.
//! - **Unix**: prefer Unix sockets for QMP. The MVP is developed and tested on
//!   Windows, so only the TCP path is exercised by the test suite.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// QMP (QEMU Machine Protocol) endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QmpEndpoint {
    Tcp { host: String, port: u16 },
    #[cfg(unix)]
    UnixSocket { path: PathBuf },
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
            QmpEndpoint::UnixSocket { path } => {
                format!("unix:{},server=on,wait=off", path.display())
            }
        }
    }
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
}
