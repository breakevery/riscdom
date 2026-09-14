//! RISC-V virtual machine lifecycle: QEMU process + serial capture.
//!
//! Verified happy-path invocation (see repo `ENVIRONMENT.md`):
//!
//! ```text
//! qemu-system-riscv64 -machine virt -cpu rv64 -m <mem>M -bios none \
//!   -display none -kernel <elf> \
//!   -qmp    tcp:127.0.0.1:<qmp>,server=on,wait=off \
//!   -serial tcp:127.0.0.1:<ser>,server=on,wait=on
//! ```
//!
//! `-bios none` is required so the guest ELF entry at `0x80000000` runs
//! directly (the default OpenSBI firmware would occupy `0x80000000`).

use crate::audit_sink::{AuditEvent, AuditSink};
use crate::error::SandboxError;
use crate::platform::{QmpEndpoint, SerialEndpoint};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Actor name used for all sandbox-originated audit events.
const AUDIT_ACTOR: &str = "sandbox";

/// How long to wait for QEMU to open its endpoints / greet QMP.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Virtual machine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMConfig {
    /// Path to the RISC-V bare-metal ELF to boot.
    pub kernel: PathBuf,
    /// Guest RAM in megabytes.
    pub memory_mb: u32,
    /// QMP endpoint.
    pub qmp: QmpEndpoint,
    /// Serial (UART) endpoint.
    pub serial: SerialEndpoint,
    /// Directory used to persist snapshots (see stage 3b fallback).
    pub snapshot_dir: PathBuf,
}

/// A running (or startable) RISC-V virtual machine.
pub struct RiscVVirtualMachine {
    config: VMConfig,
    audit: Arc<dyn AuditSink>,
    qemu_bin: PathBuf,

    child: Option<Child>,
    /// QMP connection, kept open after the greeting for later commands.
    qmp: Option<TcpStream>,
    /// Write half of the serial connection (TCP endpoints only).
    serial_write: Option<TcpStream>,
    /// Shared serial capture buffer.
    serial_buf: Arc<Mutex<Vec<u8>>>,
    /// Serial reader thread.
    reader: Option<JoinHandle<()>>,
}

impl RiscVVirtualMachine {
    /// Create a new VM handle. Does not spawn anything yet.
    pub fn new(config: VMConfig, audit: Arc<dyn AuditSink>) -> Result<Self, SandboxError> {
        if !config.kernel.exists() {
            return Err(SandboxError::Config(format!(
                "kernel not found: {}",
                config.kernel.display()
            )));
        }
        Ok(Self {
            config,
            audit,
            qemu_bin: resolve_qemu_binary(),
            child: None,
            qmp: None,
            serial_write: None,
            serial_buf: Arc::new(Mutex::new(Vec::new())),
            reader: None,
        })
    }

    /// Immutable access to the active configuration.
    pub fn config(&self) -> &VMConfig {
        &self.config
    }

    /// Whether the QEMU process is currently alive.
    pub fn is_running(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Launch QEMU, attach the serial reader, and wait for the QMP greeting.
    pub fn start(&mut self) -> Result<(), SandboxError> {
        if self.child.is_some() {
            return Err(SandboxError::AlreadyRunning);
        }

        let args = self.qemu_args();
        let mut command = Command::new(&self.qemu_bin);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = command
            .spawn()
            .map_err(|e| SandboxError::Spawn(format!("{} ({e})", self.qemu_bin.display())))?;
        self.child = Some(child);

        // 1. Serial: connect first (QEMU blocks on the serial socket while
        //    `wait=on`, so this also gates guest start-up), then attach a
        //    reader thread.
        if let SerialEndpoint::Tcp { host, port } = &self.config.serial {
            let addr = format!("{host}:{port}");
            let stream = connect_with_retry(&addr, CONNECT_TIMEOUT)?;
            let writer = stream
                .try_clone()
                .map_err(|e| SandboxError::Serial(e.to_string()))?;
            self.serial_write = Some(writer);

            let buf = Arc::clone(&self.serial_buf);
            let audit = Arc::clone(&self.audit);
            let mut read_stream = stream;
            self.reader = Some(std::thread::spawn(move || {
                let mut chunk = [0u8; 1024];
                loop {
                    match read_stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut guard) = buf.lock() {
                                guard.extend_from_slice(&chunk[..n]);
                            }
                            audit.record(AuditEvent::now(
                                AUDIT_ACTOR,
                                "serial.read",
                                serde_json::json!({ "bytes": n }),
                            ));
                        }
                        Err(_) => break,
                    }
                }
            }));
        }

        // 2. QMP: connect and consume the greeting before returning.
        match &self.config.qmp {
            QmpEndpoint::Tcp { host, port } => {
                let stream = connect_with_retry(&format!("{host}:{port}"), CONNECT_TIMEOUT)?;
                let mut stream = stream;
                stream
                    .set_read_timeout(Some(CONNECT_TIMEOUT))
                    .map_err(|e| SandboxError::Qmp(e.to_string()))?;
                read_qmp_greeting(&mut stream)?;
                self.qmp = Some(stream);
            }
            #[cfg(unix)]
            QmpEndpoint::UnixSocket { .. } => {
                return Err(SandboxError::Unsupported(
                    "QMP over Unix socket is not implemented in the MVP; use Tcp".into(),
                ));
            }
        }

        self.audit.record(AuditEvent::now(
            AUDIT_ACTOR,
            "vm.start",
            serde_json::json!({
                "kernel": self.config.kernel.display().to_string(),
                "memory_mb": self.config.memory_mb,
                "qemu": self.qemu_bin.display().to_string(),
                "args": args,
            }),
        ));

        Ok(())
    }

    /// Stop QEMU (stage 3a: terminate the process; QMP `quit` arrives in 3b).
    pub fn stop(&mut self) -> Result<(), SandboxError> {
        let was_running = self.child.is_some();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.qmp = None;
        self.serial_write = None;
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }
        if was_running {
            self.audit.record(AuditEvent::now(
                AUDIT_ACTOR,
                "vm.stop",
                serde_json::json!({}),
            ));
        }
        Ok(())
    }

    /// Snapshot of everything captured from the UART so far.
    ///
    /// For TCP endpoints this is the in-memory buffer filled by the reader
    /// thread. For file endpoints the file is read back.
    pub fn serial_output(&self) -> Vec<u8> {
        match &self.config.serial {
            SerialEndpoint::Tcp { .. } => self
                .serial_buf
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default(),
            SerialEndpoint::File { path } => std::fs::read(path).unwrap_or_default(),
        }
    }

    /// Send bytes to the guest UART.
    pub fn send_serial(&mut self, data: &[u8]) -> Result<(), SandboxError> {
        match &self.config.serial {
            SerialEndpoint::Tcp { .. } => {
                let writer = self
                    .serial_write
                    .as_mut()
                    .ok_or(SandboxError::NotRunning)?;
                writer
                    .write_all(data)
                    .map_err(|e| SandboxError::Serial(e.to_string()))?;
                let _ = writer.flush();
            }
            SerialEndpoint::File { path } => {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| SandboxError::Serial(e.to_string()))?;
                file.write_all(data)
                    .map_err(|e| SandboxError::Serial(e.to_string()))?;
            }
        }
        self.audit.record(AuditEvent::now(
            AUDIT_ACTOR,
            "serial.write",
            serde_json::json!({ "bytes": data.len() }),
        ));
        Ok(())
    }

    /// Build the QEMU command-line arguments (excluding argv[0]).
    fn qemu_args(&self) -> Vec<String> {
        vec![
            "-machine".into(),
            "virt".into(),
            "-cpu".into(),
            "rv64".into(),
            "-m".into(),
            format!("{}M", self.config.memory_mb),
            "-bios".into(),
            "none".into(),
            "-display".into(),
            "none".into(),
            "-kernel".into(),
            self.config.kernel.display().to_string(),
            "-qmp".into(),
            self.config.qmp.to_qemu_arg(),
            "-serial".into(),
            self.config.serial.to_qemu_arg(),
        ]
    }
}

impl Drop for RiscVVirtualMachine {
    fn drop(&mut self) {
        // Best-effort cleanup so a panicking test never leaks a QEMU process.
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Locate the `qemu-system-riscv64` binary.
///
/// Order: `RISCDOM_QEMU` env var → PATH → platform default install path.
fn resolve_qemu_binary() -> PathBuf {
    if let Ok(p) = std::env::var("RISCDOM_QEMU") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let exe = if cfg!(windows) {
        "qemu-system-riscv64.exe"
    } else {
        "qemu-system-riscv64"
    };
    for candidate in platform_qemu_candidates(exe) {
        if candidate.exists() {
            return candidate;
        }
    }
    // Fall back to PATH resolution by name.
    PathBuf::from(exe)
}

fn platform_qemu_candidates(exe: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            out.push(PathBuf::from(pf).join("qemu").join(exe));
        }
        out.push(PathBuf::from(r"C:\Program Files\qemu").join(exe));
    } else {
        out.push(PathBuf::from("/usr/bin").join(exe));
        out.push(PathBuf::from("/usr/local/bin").join(exe));
    }
    out
}

/// Connect a TCP stream, retrying until `timeout` elapses.
fn connect_with_retry(addr: &str, timeout: Duration) -> Result<TcpStream, SandboxError> {
    let deadline = Instant::now() + timeout;
    loop {
        match TcpStream::connect(addr) {
            Ok(stream) => return Ok(stream),
            Err(e) => {
                if Instant::now() >= deadline {
                    return Err(SandboxError::Timeout(format!(
                        "connecting to {addr}: {e}"
                    )));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Read from a QMP stream until the `{"QMP": ...}` greeting line arrives.
fn read_qmp_greeting(stream: &mut TcpStream) -> Result<(), SandboxError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    while Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => {
                return Err(SandboxError::Qmp(
                    "connection closed before QMP greeting".into(),
                ))
            }
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.windows(5).any(|w| w == b"\"QMP\"") {
                    return Ok(());
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue
            }
            Err(e) => return Err(SandboxError::Qmp(e.to_string())),
        }
    }
    Err(SandboxError::Timeout("waiting for QMP greeting".into()))
}
