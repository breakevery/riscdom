//! A local TCP relay used to turn QEMU's TCP migration stream into a file.
//!
//! Background: on Windows (+ QEMU 11.1.0) the `file:` and `exec:` migration
//! transports are unavailable (`Failed to set FD nonblocking`), while `tcp:`
//! works. This relay bridges the gap:
//!
//! - **save**: QEMU connects to us, we stream its bytes into a file
//!   ([`MigrationRelay::receive_to_file`]);
//! - **restore**: QEMU connects to us (`-incoming tcp:...`), we stream the file
//!   into it ([`MigrationRelay::send_file`]).
//!
//! Only `std` is used (`net` / `fs` / `thread`), and the listener binds to
//! `127.0.0.1:0`, so nothing is exposed off-host.

use crate::error::SandboxError;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

/// Default wait for the peer to connect / finish.
pub const DEFAULT_RELAY_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to sleep between accept attempts.
const ACCEPT_POLL: Duration = Duration::from_millis(20);

/// A one-shot TCP relay bound to a loopback port.
pub struct MigrationRelay {
    listener: TcpListener,
    addr: SocketAddr,
    timeout: Duration,
}

impl MigrationRelay {
    /// Bind to `127.0.0.1:0` (an OS-assigned port) with the default timeout.
    pub fn bind_local() -> Result<Self, SandboxError> {
        Self::bind_local_with_timeout(DEFAULT_RELAY_TIMEOUT)
    }

    /// Bind with an explicit timeout (tests use a short one).
    pub fn bind_local_with_timeout(timeout: Duration) -> Result<Self, SandboxError> {
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|e| SandboxError::Relay(e.to_string()))?;
        let addr = listener
            .local_addr()
            .map_err(|e| SandboxError::Relay(e.to_string()))?;
        Ok(Self {
            listener,
            addr,
            timeout,
        })
    }

    /// The address QEMU should migrate to / come from.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Accept the peer's stream, then write everything it sends into `path`.
    ///
    /// Returns the number of bytes written.
    pub fn receive_to_file(self, path: &Path) -> Result<u64, SandboxError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SandboxError::Io(e.to_string()))?;
        }
        let mut stream = self.accept()?;
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|e| SandboxError::Relay(e.to_string()))?;

        let mut file = File::create(path).map_err(|e| SandboxError::Io(e.to_string()))?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    file.write_all(&buf[..n])
                        .map_err(|e| SandboxError::Io(e.to_string()))?;
                    total += n as u64;
                }
                Err(ref e)
                    if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
                {
                    return Err(SandboxError::RelayTimeout(format!(
                        "receiving migration stream (got {total} bytes)"
                    )))
                }
                Err(e) => return Err(SandboxError::Relay(e.to_string())),
            }
        }
        file.flush().map_err(|e| SandboxError::Io(e.to_string()))?;
        Ok(total)
    }

    /// Accept the peer's stream, then send the contents of `path` to it.
    ///
    /// Returns the number of bytes sent.
    pub fn send_file(self, path: &Path) -> Result<u64, SandboxError> {
        // Open first: a missing file must fail before we block on accept.
        let mut file = File::open(path).map_err(|e| SandboxError::Io(e.to_string()))?;
        let mut stream = self.accept()?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|e| SandboxError::Relay(e.to_string()))?;

        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| SandboxError::Io(e.to_string()))?;
            if n == 0 {
                break;
            }
            match stream.write_all(&buf[..n]) {
                Ok(()) => total += n as u64,
                Err(ref e)
                    if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut =>
                {
                    return Err(SandboxError::RelayTimeout(format!(
                        "sending migration stream (sent {total} bytes)"
                    )))
                }
                Err(e) => return Err(SandboxError::Relay(e.to_string())),
            }
        }
        let _ = stream.flush();
        // Signal EOF so QEMU knows the stream is complete.
        let _ = stream.shutdown(Shutdown::Write);
        Ok(total)
    }

    /// Accept one connection, honouring this relay's timeout.
    fn accept(&self) -> Result<TcpStream, SandboxError> {
        self.listener
            .set_nonblocking(true)
            .map_err(|e| SandboxError::Relay(e.to_string()))?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match self.listener.accept() {
                Ok((stream, _peer)) => {
                    stream
                        .set_nonblocking(false)
                        .map_err(|e| SandboxError::Relay(e.to_string()))?;
                    return Ok(stream);
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(SandboxError::RelayTimeout(
                            "waiting for the peer to connect".to_string(),
                        ));
                    }
                    std::thread::sleep(ACCEPT_POLL);
                }
                Err(e) => return Err(SandboxError::Relay(e.to_string())),
            }
        }
    }
}
