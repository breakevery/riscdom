//! RISC-V virtual machine lifecycle: QEMU process + serial capture + QMP.
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

use audit::{AuditEvent, AuditSink};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::SandboxError;
use crate::platform::{connect_with_retry, QmpEndpoint, SerialEndpoint};
use crate::qmp::QmpClient;

/// Actor name used for all sandbox-originated audit events.
const AUDIT_ACTOR: &str = "sandbox";

/// How long to wait for QEMU to open its endpoints / greet QMP.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long to wait for QEMU to exit after a QMP `quit` before force-killing.
const QUIT_GRACE: Duration = Duration::from_secs(2);

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
    /// Directory used to persist snapshots (see the fallback in
    /// [`RiscVVirtualMachine::save_snapshot`]).
    pub snapshot_dir: PathBuf,
}

/// A running (or startable) RISC-V virtual machine.
pub struct RiscVVirtualMachine {
    config: VMConfig,
    audit: Arc<Mutex<dyn AuditSink>>,
    qemu_bin: PathBuf,

    child: Option<Child>,
    /// QMP connection, kept open for `stop` / `cont` / `quit`.
    qmp: Option<QmpClient>,
    /// Write half of the serial connection (TCP endpoints only).
    serial_write: Option<TcpStream>,
    /// Shared serial capture buffer.
    serial_buf: Arc<Mutex<Vec<u8>>>,
    /// Serial reader thread.
    reader: Option<JoinHandle<()>>,
}

impl RiscVVirtualMachine {
    /// Create a new VM handle. Does not spawn anything yet.
    pub fn new(config: VMConfig, audit: Arc<Mutex<dyn AuditSink>>) -> Result<Self, SandboxError> {
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

    /// Launch QEMU, attach the serial reader, and negotiate QMP.
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
                            if let Ok(mut sink) = audit.lock() {
                                sink.record(AuditEvent::new(
                                    AUDIT_ACTOR,
                                    "serial.read",
                                    serde_json::json!({ "bytes": n }),
                                ));
                            }
                        }
                        Err(_) => break,
                    }
                }
            }));
        }

        // 2. QMP: connect, consume greeting, negotiate capabilities.
        self.qmp = Some(QmpClient::connect(&self.config.qmp, CONNECT_TIMEOUT)?);

        self.emit(
            "vm.start",
            serde_json::json!({
                "kernel": self.config.kernel.display().to_string(),
                "memory_mb": self.config.memory_mb,
                "qemu": self.qemu_bin.display().to_string(),
                "args": args,
            }),
        );

        Ok(())
    }

    /// Stop the VM: ask QEMU to quit over QMP, then force-kill if needed.
    pub fn stop(&mut self) -> Result<(), SandboxError> {
        let was_running = self.child.is_some();

        if let Some(qmp) = self.qmp.as_mut() {
            let _ = qmp.quit();
        }

        if let Some(mut child) = self.child.take() {
            let deadline = Instant::now() + QUIT_GRACE;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if Instant::now() >= deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => {
                        let _ = child.kill();
                        break;
                    }
                }
            }
        }

        self.qmp = None;
        self.serial_write = None;
        if let Some(handle) = self.reader.take() {
            let _ = handle.join();
        }

        if was_running {
            self.emit("vm.stop", serde_json::json!({}));
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
                let writer = self.serial_write.as_mut().ok_or(SandboxError::NotRunning)?;
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
        self.emit("serial.write", serde_json::json!({ "bytes": data.len() }));
        Ok(())
    }

    /// Save a snapshot.
    ///
    /// **MVP fallback, not real VM state capture.** This serializes the boot
    /// parameters (kernel path, memory, QEMU args, timestamp) to
    /// `<snapshot_dir>/<name>.json`. Recovering it means stopping the current
    /// VM and re-booting with those parameters.
    ///
    /// v0.2 will replace this with QEMU `savevm` / `loadvm` over QMP, which
    /// captures true device + memory state.
    pub fn save_snapshot(&mut self, name: &str) -> Result<(), SandboxError> {
        std::fs::create_dir_all(&self.config.snapshot_dir)
            .map_err(|e| SandboxError::Snapshot(e.to_string()))?;
        let path = self.snapshot_path(name);
        let snapshot = serde_json::json!({
            "name": name,
            "timestamp_ms": now_ms(),
            "mode": "mvp-reboot",
            "config": self.config,
            "qemu_args": self.qemu_args(),
        });
        let text = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| SandboxError::Snapshot(e.to_string()))?;
        std::fs::write(&path, text).map_err(|e| SandboxError::Snapshot(e.to_string()))?;

        self.emit(
            "vm.snapshot.save",
            serde_json::json!({
                "name": name,
                "path": path.display().to_string(),
                "mode": "mvp-reboot",
            }),
        );
        Ok(())
    }

    /// Restore a snapshot.
    ///
    /// **MVP fallback, not real VM state restore.** Stops any running VM and
    /// re-boots using the parameters stored by [`Self::save_snapshot`].
    ///
    /// v0.2 will use QEMU `loadvm`.
    pub fn load_snapshot(&mut self, name: &str) -> Result<(), SandboxError> {
        let path = self.snapshot_path(name);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| SandboxError::Snapshot(format!("{}: {e}", path.display())))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| SandboxError::Snapshot(e.to_string()))?;
        let stored = value
            .get("config")
            .cloned()
            .ok_or_else(|| SandboxError::Snapshot("snapshot missing 'config'".into()))?;
        let config: VMConfig =
            serde_json::from_value(stored).map_err(|e| SandboxError::Snapshot(e.to_string()))?;

        // Reboot with the stored parameters.
        self.stop()?;
        self.config = config;
        self.serial_buf = Arc::new(Mutex::new(Vec::new()));
        self.start()?;

        self.emit(
            "vm.snapshot.load",
            serde_json::json!({
                "name": name,
                "path": path.display().to_string(),
                "mode": "mvp-reboot",
            }),
        );
        Ok(())
    }

    fn snapshot_path(&self, name: &str) -> PathBuf {
        self.config.snapshot_dir.join(format!("{name}.json"))
    }

    /// Record an audit event (best-effort: never fails the caller).
    fn emit(&self, action: &str, detail: serde_json::Value) {
        if let Ok(mut sink) = self.audit.lock() {
            sink.record(AuditEvent::new(AUDIT_ACTOR, action, detail));
        }
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

/// Milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Locate the `qemu-system-riscv64` binary.
///
/// Order: `RISCDOM_QEMU` env var → common install paths → PATH.
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
