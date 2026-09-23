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
use std::collections::HashSet;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Default wait for the peer to connect / finish.
pub const DEFAULT_RELAY_TIMEOUT: Duration = Duration::from_secs(60);

/// How long to sleep between accept attempts.
const ACCEPT_POLL: Duration = Duration::from_millis(20);

/// How many binds one request makes before reporting the ports as exhausted.
pub const MAX_LEASE_ATTEMPTS: usize = 64;

/// The ports this process has handed out and not yet released (v0.4 #1).
///
/// A **set**, because the contract is set membership: a number is either reserved
/// by this process or it is not, and a lease's release removes its own number once.
/// (`HashSet::new` cannot initialise a `static` — its hasher wants a runtime seed —
/// hence the `LazyLock`.)
static HELD_PORTS: LazyLock<Mutex<HashSet<u16>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// A loopback port reserved by this process.
///
/// **The contract is about *live* leases**: two leases that exist at the same time
/// never carry the same number. It is deliberately not "a number is never handed
/// out twice in this process": once the holder is gone (or has handed the port
/// off), the number goes back to the OS pool and is free to come back — and the
/// callers' retries exist precisely because the peer binds only after we let go.
///
/// Until [`PortLease::hand_off`] the lease also keeps a *listener* bound, so the
/// OS cannot give the port to anyone else; the number then stays reserved here
/// until the lease is dropped.
///
/// What this does and does not buy: two parts of *this* program can no longer be
/// handed the same port, and the OS holds the port until the caller hands it off.
/// It cannot make the hand-off itself atomic — the peer still binds only after we
/// let go, because QEMU (with today's flags) cannot be given a pre-bound socket.
/// So the retries stay, as the backstop for the window that is left.
pub struct PortLease {
    port: u16,
    listener: Option<TcpListener>,
}

impl PortLease {
    /// The reserved port number.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether the OS-level hold (the bound listener) is still in place.
    pub fn holds_listener(&self) -> bool {
        self.listener.is_some()
    }

    /// Release the listener so the peer can bind the port itself.
    ///
    /// Call this as late as possible — immediately before the peer is started —
    /// because until it happens the port cannot be taken by anyone. The *number*
    /// stays reserved in this process until the lease is dropped, so our own
    /// allocator cannot hand the same port to a second holder in the meantime.
    pub fn hand_off(&mut self) {
        self.listener = None;
    }
}

impl Drop for PortLease {
    fn drop(&mut self) {
        self.listener = None;
        if let Ok(mut held) = HELD_PORTS.lock() {
            // This lease's own number, and only it: a release must never take
            // another holder's reservation with it.
            held.remove(&self.port);
        }
    }
}

impl std::fmt::Debug for PortLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortLease")
            .field("port", &self.port)
            .field("holds_listener", &self.listener.is_some())
            .finish()
    }
}

/// Record `port` as held by this process. `false` when someone else has it.
///
/// One lock scope, one insert: the check and the record are the same operation, so
/// there is no window between them for a second holder to slip through.
fn reserve(port: u16) -> bool {
    match HELD_PORTS.lock() {
        Ok(mut held) => held.insert(port),
        // A poisoned registry means a holder panicked; refusing the port is the
        // safe answer.
        Err(_) => false,
    }
}

/// The ports this process currently reserves (diagnostics and tests).
///
/// Unordered: it is a set, and no caller has ever depended on an order.
pub fn leased_ports() -> Vec<u16> {
    HELD_PORTS
        .lock()
        .map(|held| held.iter().copied().collect())
        .unwrap_or_default()
}

/// Reserve `count` distinct loopback ports for this process (v0.4 #1).
///
/// Replaces the old `free_local_port`: bind `127.0.0.1:0`, take the port the OS
/// assigned, and **keep** it reserved — together with its listener — until the
/// returned leases are dropped or handed off.
///
/// Returns [`SandboxError::PortLease`] when no port could be reserved after
/// [`MAX_LEASE_ATTEMPTS`] tries, naming how many are already held.
pub fn lease_local_ports(count: usize) -> Result<Vec<PortLease>, SandboxError> {
    lease_local_ports_with_attempts(count, MAX_LEASE_ATTEMPTS)
}

/// [`lease_local_ports`] with an explicit bind budget **per port**.
///
/// Exposed so a caller can fail fast, and so the exhaustion path has a
/// deterministic test instead of one that waits for bad luck.
pub fn lease_local_ports_with_attempts(
    count: usize,
    attempts: usize,
) -> Result<Vec<PortLease>, SandboxError> {
    let mut leases: Vec<PortLease> = Vec::with_capacity(count);
    for _ in 0..count {
        let mut reserved = None;
        for _ in 0..attempts {
            let listener = TcpListener::bind("127.0.0.1:0")
                .map_err(|e| SandboxError::PortLease(e.to_string()))?;
            let port = listener
                .local_addr()
                .map_err(|e| SandboxError::PortLease(e.to_string()))?
                .port();
            // The OS will not hand out a port that is bound right now, but it can
            // hand out one this very call released a moment ago, so check too.
            if leases.iter().any(|lease| lease.port == port) {
                continue;
            }
            if reserve(port) {
                reserved = Some(PortLease {
                    port,
                    listener: Some(listener),
                });
                break;
            }
        }
        match reserved {
            Some(lease) => leases.push(lease),
            None => {
                return Err(SandboxError::PortLease(format!(
                    "no free loopback port after {attempts} attempts \
                     ({} already reserved by this process)",
                    leases.len()
                )))
            }
        }
    }
    Ok(leases)
}

/// Reserve one loopback port. See [`lease_local_ports`].
pub fn lease_local_port() -> Result<PortLease, SandboxError> {
    let mut leases = lease_local_ports(1)?;
    Ok(leases.remove(0))
}

/// Stream a file into a **listening** peer (client mode).
///
/// QEMU's `-incoming tcp:host:port` *listens*, so restoring means connecting to
/// it and pushing the snapshot bytes (the mirror image of [`MigrationRelay`]).
pub fn send_file_to(addr: SocketAddr, path: &Path, timeout: Duration) -> Result<u64, SandboxError> {
    let mut file = File::open(path).map_err(|e| SandboxError::Io(e.to_string()))?;

    let deadline = Instant::now() + timeout;
    let mut stream = loop {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
            Ok(stream) => break stream,
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(SandboxError::RelayTimeout(format!(
                        "connecting to the incoming peer at {addr}: {e}"
                    )));
                }
                std::thread::sleep(ACCEPT_POLL);
            }
        }
    };
    stream
        .set_write_timeout(Some(timeout))
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
        stream
            .write_all(&buf[..n])
            .map_err(|e| SandboxError::Relay(e.to_string()))?;
        total += n as u64;
    }
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Write);
    Ok(total)
}

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
    /// Runs until the peer closes the connection. Returns the number of bytes
    /// written.
    pub fn receive_to_file(self, path: &Path) -> Result<u64, SandboxError> {
        self.receive_inner(path, None)
    }

    /// Like [`Self::receive_to_file`], but also stops as soon as `stop` is set.
    ///
    /// QEMU does not always close the migration socket once the transfer has
    /// finished, so the caller sets `stop` when QMP reports `completed`.
    pub fn receive_to_file_until(
        self,
        path: &Path,
        stop: Arc<AtomicBool>,
    ) -> Result<u64, SandboxError> {
        self.receive_inner(path, Some(stop))
    }

    fn receive_inner(
        self,
        path: &Path,
        stop: Option<Arc<AtomicBool>>,
    ) -> Result<u64, SandboxError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SandboxError::Io(e.to_string()))?;
        }
        let mut stream = self.accept()?;
        // Short polls so `stop` is honoured promptly.
        let poll = Duration::from_millis(200).min(self.timeout);
        stream
            .set_read_timeout(Some(poll))
            .map_err(|e| SandboxError::Relay(e.to_string()))?;

        let mut file = File::create(path).map_err(|e| SandboxError::Io(e.to_string()))?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        let deadline = Instant::now() + self.timeout;
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
                    if let Some(stop) = &stop {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    if Instant::now() >= deadline {
                        return Err(SandboxError::RelayTimeout(format!(
                            "receiving migration stream (got {total} bytes)"
                        )));
                    }
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

#[cfg(test)]
mod lease_tests {
    use super::*;

    #[test]
    fn exhaustion_is_reported_clearly() {
        let error = lease_local_ports_with_attempts(1, 0).expect_err("no attempts, no port");
        let text = error.to_string();
        assert!(text.contains("no free loopback port"), "{text}");
        assert!(text.contains("already reserved by this process"), "{text}");
    }
}
