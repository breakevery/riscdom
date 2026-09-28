//! The transport: one JSON line per message over TCP (v1.0 M4a).
//!
//! [connection.md §3.1](../../docs/connection.md) freezes this. A signed message moves as
//! **one JSON line over a TCP connection** — the discipline `worker` and the plugin
//! interface already use — sent **direct first** and, when that fails, handed to a
//! **relay**. The frame is **byte-identical on both paths**, which is what keeps the relay
//! stateless: it forwards a line it cannot usefully alter, because the signature covers
//! `from`, `to`, `v` and `ts`. The connection is plaintext, with integrity coming from the
//! signature ([security-model.md §3](../../docs/security-model.md)); no separate handshake exists,
//! because `v` is checked per message; and the server side never dials a node, which is why
//! this project needs no hole punching.
//!
//! **What is here, and what is not.** Direct sending and receiving are implemented with
//! **`std::net`**, not an async runtime: `sandbox/relay.rs` already proves std is enough
//! for a small framed protocol, and pulling a runtime in would make every caller pay for
//! a scheduler it does not use. The relay's *routing* is M4d's and is **not** here: this
//! module fixes the seam ([`Relay`]) and the direct path, so the two cannot disagree about
//! what a frame is.
//!
//! **Ports and timeouts are not frozen** (§3.1 leaves them to the implementation), so they
//! live in [`TransportConfig`] with working defaults.

use crate::error::Category;
use crate::message::{MessageError, SignedMessage};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// The byte that ends a frame. §3.1's "one JSON line": the newline is the boundary.
pub const FRAME_TERMINATOR: u8 = b'\n';

/// The bytes of one frame: the line's own bytes, then the terminator when the caller did
/// not already include it.
///
/// **The one place a frame is framed.** [`Connection::send_frame`] and the cross-region
/// server's push down a session ([`crate::relay`]) both go through it, so §3.1's
/// "byte-identical on both paths" is held by there being one encoder rather than by two
/// call sites agreeing.
pub fn frame_bytes(frame: &str) -> Vec<u8> {
    let mut bytes = frame.as_bytes().to_vec();
    if !bytes.ends_with(&[FRAME_TERMINATOR]) {
        bytes.push(FRAME_TERMINATOR);
    }
    bytes
}

/// Connect timeout, when the caller does not choose one.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Read timeout: a connection that cannot deliver a **complete** frame in this long is
/// closed (§3.1).
pub const DEFAULT_READ_TIMEOUT: Duration = Duration::from_secs(10);
/// Write timeout.
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest frame this build will read. An implementation limit, not a protocol rule:
/// §3.1 freezes the framing, not a size. It exists so a peer cannot make the reader
/// allocate without bound.
pub const DEFAULT_MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Ports and timeouts, which §3.1 deliberately does not freeze.
#[derive(Debug, Clone, Copy)]
pub struct TransportConfig {
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
    pub max_frame_bytes: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            read_timeout: DEFAULT_READ_TIMEOUT,
            write_timeout: DEFAULT_WRITE_TIMEOUT,
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
        }
    }
}

/// Which half of the transport failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Connect,
    Write,
    Read,
    Accept,
}

impl Op {
    fn as_str(self) -> &'static str {
        match self {
            Op::Connect => "connect",
            Op::Write => "write",
            Op::Read => "read",
            Op::Accept => "accept",
        }
    }
}

/// Why a frame did not move, and which error-model category says so.
#[derive(Debug)]
pub enum TransportError {
    /// A read or write ran out of time. §3.1: `network`.
    Timeout { op: Op, after: Duration },
    /// The socket refused for some other reason — refused, unreachable, reset. §3.1:
    /// `network`.
    Io { op: Op, why: String },
    /// The connection ended before a complete frame arrived — a clean EOF or a line cut
    /// short. §3.1: `network`, because whether the peer crashed is not something a frame
    /// can tell us.
    Closed { op: Op },
    /// A line longer than [`TransportConfig::max_frame_bytes`]: refused rather than read,
    /// which is this build's limit and not a protocol rule.
    TooLong { limit: usize },
    /// A complete frame arrived and does not parse. §3.1: `invalid`.
    Malformed { why: String },
    /// The direct path failed and **nothing is wired to carry a frame instead**.
    ///
    /// This is where M4a leaves the relay: the seam is [`Relay`], and a deployment with
    /// no relay gets this rather than a silent drop. It is `network` because nothing was
    /// *reached* — `refused` is for a peer that answered.
    RelayUnavailable { after: String },
}

impl TransportError {
    /// The error model's category for this failure — §3.1's table, applied.
    pub fn category(&self) -> Category {
        match self {
            TransportError::Timeout { .. }
            | TransportError::Io { .. }
            | TransportError::Closed { .. }
            | TransportError::TooLong { .. }
            | TransportError::RelayUnavailable { .. } => Category::Network,
            TransportError::Malformed { .. } => Category::Invalid,
        }
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Timeout { op, after } => {
                write!(f, "the {} timed out after {after:?}", op.as_str())
            }
            TransportError::Io { op, why } => write!(f, "the {} failed: {why}", op.as_str()),
            TransportError::Closed { op } => write!(
                f,
                "the connection ended before a complete frame arrived ({})",
                op.as_str()
            ),
            TransportError::TooLong { limit } => {
                write!(f, "the frame is longer than the {limit}-byte limit")
            }
            TransportError::Malformed { why } => write!(f, "the frame does not parse: {why}"),
            TransportError::RelayUnavailable { after } => write!(
                f,
                "the direct path failed ({after}) and no relay is wired to carry the frame"
            ),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<MessageError> for TransportError {
    fn from(error: MessageError) -> Self {
        TransportError::Malformed {
            why: error.to_string(),
        }
    }
}

/// The relay's half of §3.1: hand one frame to something that will carry it.
///
/// The seam is deliberately this small. A relay forwards a **line it cannot usefully
/// alter** — the signature covers `from`, `to`, `v` and `ts`, so a relay that rewrote a
/// frame would break it — and *how* it routes, who may ask it to, and how a node learns
/// where it is, are M4d's (the frozen document's §6.3). **M4a does not implement any of
/// that**: it defines the trait, sends through it when the direct path fails, and leaves
/// [`NoRelay`] as the honest answer for a deployment with nothing wired.
pub trait Relay: Send + Sync {
    /// Forward `frame` — the exact bytes the direct path would have written — towards
    /// the node it is addressed to.
    fn forward(&self, frame: &str) -> Result<(), TransportError>;

    /// A short description, for an error message or a log line.
    fn describe(&self) -> String {
        "a relay".to_string()
    }
}

/// Nothing is wired to relay. The answer M4d will replace.
pub struct NoRelay;

impl Relay for NoRelay {
    fn forward(&self, _frame: &str) -> Result<(), TransportError> {
        Err(TransportError::RelayUnavailable {
            after: "no relay is configured".to_string(),
        })
    }

    fn describe(&self) -> String {
        "no relay".to_string()
    }
}

/// Which path carried a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Path {
    /// The peer's own address took it.
    Direct,
    /// The direct attempt failed and the relay carried it.
    Relayed { after: String },
}

/// A listening socket: what the receiving half of the transport sits on.
pub struct Listener {
    inner: TcpListener,
    config: TransportConfig,
}

impl Listener {
    /// Bind with the default configuration. `127.0.0.1:0` picks any free loopback port,
    /// which is how the tests and the self-test get one.
    pub fn bind(addr: impl ToSocketAddrs) -> Result<Self, TransportError> {
        Self::bind_with(addr, TransportConfig::default())
    }

    /// Bind with an explicit configuration.
    pub fn bind_with(
        addr: impl ToSocketAddrs,
        config: TransportConfig,
    ) -> Result<Self, TransportError> {
        let inner = TcpListener::bind(addr).map_err(|e| TransportError::Io {
            op: Op::Connect,
            why: e.to_string(),
        })?;
        Ok(Self { inner, config })
    }

    /// The address it is listening on — the truth after a `:0` bind.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.inner.local_addr().map_err(|e| TransportError::Io {
            op: Op::Accept,
            why: e.to_string(),
        })
    }

    /// Wait for one peer and answer a connection with its timeouts already set.
    pub fn accept(&self) -> Result<Connection, TransportError> {
        let (stream, peer) = self.inner.accept().map_err(|e| TransportError::Io {
            op: Op::Accept,
            why: e.to_string(),
        })?;
        Connection::from_stream(stream, peer, self.config)
    }

    /// The configuration new connections inherit.
    pub fn config(&self) -> TransportConfig {
        self.config
    }
}

/// One connection, carrying messages one JSON line at a time.
///
/// A connection may carry **several** messages — §3.1 sends one request at a time and
/// answers in order, so a reader takes one frame per call — and it is the caller's to
/// close.
pub struct Connection {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    peer: SocketAddr,
    config: TransportConfig,
}

impl Connection {
    /// Dial a peer, with the default configuration.
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self, TransportError> {
        Self::connect_with(addr, TransportConfig::default())
    }

    /// Dial a peer, with an explicit configuration.
    pub fn connect_with(
        addr: impl ToSocketAddrs,
        config: TransportConfig,
    ) -> Result<Self, TransportError> {
        let target = addr
            .to_socket_addrs()
            .map_err(|e| TransportError::Io {
                op: Op::Connect,
                why: e.to_string(),
            })?
            .next()
            .ok_or_else(|| TransportError::Io {
                op: Op::Connect,
                why: "the address resolved to nothing".to_string(),
            })?;
        let stream = TcpStream::connect_timeout(&target, config.connect_timeout).map_err(|e| {
            match e.kind() {
                std::io::ErrorKind::TimedOut => TransportError::Timeout {
                    op: Op::Connect,
                    after: config.connect_timeout,
                },
                _ => TransportError::Io {
                    op: Op::Connect,
                    why: e.to_string(),
                },
            }
        })?;
        Self::from_stream(stream, target, config)
    }

    /// Wrap a socket that is already open — the accepting side of a connection.
    pub fn from_stream(
        stream: TcpStream,
        peer: SocketAddr,
        config: TransportConfig,
    ) -> Result<Self, TransportError> {
        stream
            .set_read_timeout(Some(config.read_timeout))
            .map_err(|e| TransportError::Io {
                op: Op::Read,
                why: e.to_string(),
            })?;
        stream
            .set_write_timeout(Some(config.write_timeout))
            .map_err(|e| TransportError::Io {
                op: Op::Write,
                why: e.to_string(),
            })?;
        let writer = stream.try_clone().map_err(|e| TransportError::Io {
            op: Op::Write,
            why: e.to_string(),
        })?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
            peer,
            config,
        })
    }

    /// The other end.
    pub fn peer(&self) -> SocketAddr {
        self.peer
    }

    /// The configuration in force.
    pub fn config(&self) -> TransportConfig {
        self.config
    }

    /// Write one message: its frame, then the newline that ends it.
    ///
    /// The bytes are [`SignedMessage::to_line`]'s, and they are the same ones
    /// [`Relay::forward`] receives — that is §3.1's "byte-identical on both paths", held
    /// by having exactly one place that serialises a frame.
    pub fn send(&mut self, message: &SignedMessage) -> Result<usize, TransportError> {
        let frame = message.to_line()?;
        self.send_frame(&frame)
    }

    /// Write a frame that is already serialised — the relay's path uses this, so a
    /// forwarded frame cannot be re-encoded on the way.
    pub fn send_frame(&mut self, frame: &str) -> Result<usize, TransportError> {
        let bytes = frame_bytes(frame);
        self.writer
            .write_all(&bytes)
            .and_then(|()| self.writer.flush())
            .map_err(|e| self.write_error(e))?;
        Ok(bytes.len())
    }

    /// A **write-only clone** of this connection's socket.
    ///
    /// The cross-region server needs it ([`crate::relay`]): a session's reader sits in a
    /// read on its own thread, while pushing a frame down that session is a write from
    /// somebody else. The clone is the same socket, so a push lands on the connection its
    /// peer dialled in on, and neither half has to hold the other's lock.
    pub fn writer_clone(&self) -> Result<TcpStream, TransportError> {
        self.writer.try_clone().map_err(|e| TransportError::Io {
            op: Op::Write,
            why: e.to_string(),
        })
    }

    /// Change the read timeout in force.
    ///
    /// The relay needs `None` for a session: §6.3 has the server wait to be dialled and
    /// the node keep the connection open, and a node may be silent between messages — a
    /// session that timed out while idle would be a session that could never be reached.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) -> Result<(), TransportError> {
        self.reader
            .get_ref()
            .set_read_timeout(timeout)
            .map_err(|e| TransportError::Io {
                op: Op::Read,
                why: e.to_string(),
            })
    }

    /// Read one message: one complete frame, then the `\n` that ends it.
    ///
    /// A connection that ends before a complete frame, or a line longer than the limit,
    /// is [`TransportError::Closed`] / [`TransportError::TooLong`] — both `network` per
    /// §3.1, and neither silently truncated into a shorter message.
    pub fn receive(&mut self) -> Result<SignedMessage, TransportError> {
        let frame = self.receive_frame()?;
        SignedMessage::parse_line(&frame).map_err(TransportError::from)
    }

    /// Read one raw frame, newline included.
    pub fn receive_frame(&mut self) -> Result<String, TransportError> {
        let mut buffer: Vec<u8> = Vec::new();
        let limit = self.config.max_frame_bytes as u64;
        let read = (&mut self.reader)
            .take(limit + 1)
            .read_until(FRAME_TERMINATOR, &mut buffer)
            .map_err(|e| self.read_error(e))?;
        if read == 0 {
            return Err(TransportError::Closed { op: Op::Read });
        }
        if !buffer.ends_with(&[FRAME_TERMINATOR]) {
            if buffer.len() as u64 > limit {
                return Err(TransportError::TooLong {
                    limit: self.config.max_frame_bytes,
                });
            }
            // A line cut short by an EOF: the connection ended before a complete frame.
            return Err(TransportError::Closed { op: Op::Read });
        }
        String::from_utf8(buffer).map_err(|e| TransportError::Malformed { why: e.to_string() })
    }

    fn read_error(&self, error: std::io::Error) -> TransportError {
        match error.kind() {
            // A read timeout is `WouldBlock` on Unix and `TimedOut` on Windows; both are
            // "the frame did not arrive in time", which §3.1 calls `network`.
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                TransportError::Timeout {
                    op: Op::Read,
                    after: self.config.read_timeout,
                }
            }
            _ => TransportError::Io {
                op: Op::Read,
                why: error.to_string(),
            },
        }
    }

    fn write_error(&self, error: std::io::Error) -> TransportError {
        match error.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
                TransportError::Timeout {
                    op: Op::Write,
                    after: self.config.write_timeout,
                }
            }
            _ => TransportError::Io {
                op: Op::Write,
                why: error.to_string(),
            },
        }
    }
}

/// Send one message **direct to the peer's address**, and nothing else.
pub fn send_direct(
    addr: impl ToSocketAddrs,
    message: &SignedMessage,
    config: TransportConfig,
) -> Result<usize, TransportError> {
    let mut connection = Connection::connect_with(addr, config)?;
    connection.send(message)
}

/// Send one message: **direct first**, and the relay when that fails (§3.1).
///
/// The frame is serialised **once**, here, and the same string goes to whichever path
/// carries it. That is the whole of "byte-identical on both paths": there is no second
/// encoder that could drift, and no re-encoding on the relay leg.
///
/// `relay` is `None` when the caller has nothing wired; the seam is [`Relay`] and M4d is
/// what implements forwarding behind it.
pub fn deliver(
    addr: impl ToSocketAddrs,
    message: &SignedMessage,
    relay: Option<&dyn Relay>,
    config: TransportConfig,
) -> Result<Path, TransportError> {
    let frame = message.to_line()?;
    match Connection::connect_with(addr, config).and_then(|mut c| c.send_frame(&frame)) {
        Ok(_) => Ok(Path::Direct),
        Err(direct) => {
            let after = direct.to_string();
            match relay {
                Some(relay) => relay.forward(&frame).map(|()| Path::Relayed { after }),
                // Nothing wired: the direct failure is what happened, and saying so beats
                // pretending the frame went somewhere.
                None => Err(TransportError::RelayUnavailable { after }),
            }
        }
    }
}
