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
use std::path::{Path, PathBuf};
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

/// How long to wait for a migration (save) or an incoming restore to finish.
const MIGRATE_TIMEOUT: Duration = Duration::from_secs(30);

/// QEMU machine the sandbox boots (`-machine`).
///
/// Exported so anything that has to *name* the machine for a run (the host's
/// configuration fingerprint) reads one authority instead of copying the
/// literal — a copy that silently goes stale when this changes (v0.4 1e).
pub const VM_MACHINE: &str = "virt";

/// CPU model the sandbox boots (`-cpu`); exported for the same reason as
/// [`VM_MACHINE`].
pub const VM_CPU: &str = "rv64";

/// A serial observer callback: receives newly-read UART bytes as they arrive.
pub type SerialObserver = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Virtual machine configuration.
#[derive(Clone, Serialize, Deserialize)]
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
    /// Optional observer called with each chunk of serial output.
    ///
    /// Not serialised (it is a callback, not data) and defaults to `None`.
    #[serde(skip)]
    pub serial_observer: Option<SerialObserver>,
    /// When set, the VM is started with `-incoming tcp:` and this migration
    /// stream file is fed to it through a local relay (see
    /// [`RiscVVirtualMachine::resume_from_snapshot_real`]).
    #[serde(default)]
    pub incoming_snapshot: Option<PathBuf>,
    /// The relay address chosen by [`RiscVVirtualMachine::start`] for
    /// `-incoming` (informational).
    #[serde(default)]
    pub incoming_relay_addr: Option<std::net::SocketAddr>,
    /// Explicit QEMU executable (v0.3 #5a). `None` → auto-discovery
    /// ([`crate::qemu_discover::discover`]).
    #[serde(default)]
    pub qemu_exe: Option<PathBuf>,
}

impl std::fmt::Debug for VMConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VMConfig")
            .field("kernel", &self.kernel)
            .field("memory_mb", &self.memory_mb)
            .field("qmp", &self.qmp)
            .field("serial", &self.serial)
            .field("snapshot_dir", &self.snapshot_dir)
            .field(
                "serial_observer",
                &self.serial_observer.as_ref().map(|_| "<observer>"),
            )
            .field("incoming_snapshot", &self.incoming_snapshot)
            .field("incoming_relay_addr", &self.incoming_relay_addr)
            .field("qemu_exe", &self.qemu_exe)
            .finish()
    }
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
    /// Thread feeding a snapshot into QEMU's `-incoming` connection.
    snapshot_sender: Option<JoinHandle<Result<u64, SandboxError>>>,
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
        let qemu_bin = config.qemu_exe.clone().unwrap_or_else(resolve_qemu_binary);
        if !qemu_bin.is_file() {
            // Actionable: the diagnostics list every location that was tried.
            return Err(SandboxError::QemuNotFound {
                diagnostics: crate::qemu_discover::diagnostics(),
            });
        }
        Ok(Self {
            config,
            audit,
            qemu_bin,
            child: None,
            qmp: None,
            serial_write: None,
            serial_buf: Arc::new(Mutex::new(Vec::new())),
            reader: None,
            snapshot_sender: None,
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

        // Incoming migration: QEMU *listens* on `-incoming tcp:<addr>`, and a
        // thread of ours connects and pushes the snapshot into it.
        if let Some(snapshot) = self.config.incoming_snapshot.clone() {
            let port = crate::relay::free_local_port()?;
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            self.config.incoming_relay_addr = Some(addr);
            self.snapshot_sender = Some(std::thread::spawn(move || {
                crate::relay::send_file_to(addr, &snapshot, crate::relay::DEFAULT_RELAY_TIMEOUT)
            }));
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
            let observer = self.config.serial_observer.clone();
            let mut read_stream = stream;
            self.reader = Some(std::thread::spawn(move || {
                let mut chunk = [0u8; 1024];
                loop {
                    match read_stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            // 1. always buffer first (unchanged behaviour)
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
                            // 2. then notify the observer (never blocks the
                            //    reader; a panic is caught and audited).
                            if let Some(obs) = observer.as_ref() {
                                let f: &dyn Fn(&[u8]) = obs.as_ref();
                                let data = chunk[..n].to_vec();
                                let outcome =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        f(&data)
                                    }));
                                if outcome.is_err() {
                                    if let Ok(mut sink) = audit.lock() {
                                        sink.record(AuditEvent::new(
                                            AUDIT_ACTOR,
                                            "sandbox.serial.observer_panic",
                                            serde_json::json!({ "bytes": n }),
                                        ));
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }));
        }

        // 2. QMP: connect, consume greeting, negotiate capabilities.
        self.qmp = Some(QmpClient::connect(&self.config.qmp, CONNECT_TIMEOUT)?);

        // 3. With `-incoming`, wait until the restored guest is actually running.
        if self.config.incoming_snapshot.is_some() {
            self.wait_for_running(MIGRATE_TIMEOUT)?;
        }

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
        if let Some(handle) = self.snapshot_sender.take() {
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

    /// Save a **real** VM state snapshot (QMP `migrate` over a local TCP relay).
    ///
    /// The stream is written to `<snapshot_dir>/<name>.mig`. Existing snapshots
    /// are **not** overwritten: an error is returned instead, so a snapshot is
    /// never silently replaced.
    ///
    /// On Windows the `file:` migration transport is unavailable, which is why
    /// the stream goes through [`crate::relay::MigrationRelay`].
    pub fn save_snapshot_real(&mut self, name: &str) -> Result<(), SandboxError> {
        std::fs::create_dir_all(&self.config.snapshot_dir)
            .map_err(|e| SandboxError::Snapshot(e.to_string()))?;
        let path = self.config.snapshot_dir.join(format!("{name}.mig"));
        if path.exists() {
            return Err(SandboxError::Snapshot(format!(
                "snapshot already exists: {} (refusing to overwrite)",
                path.display()
            )));
        }

        let relay = crate::relay::MigrationRelay::bind_local()?;
        let addr = relay.addr();

        // Receive in a thread first: QEMU only connects once `migrate` is sent.
        // QEMU may leave the socket open after finishing, so the receiver stops
        // as soon as the migration is reported complete.
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_rx = Arc::clone(&stop);
        let path_for_thread = path.clone();
        let receiver =
            std::thread::spawn(move || relay.receive_to_file_until(&path_for_thread, stop_rx));

        let migrate_result = match self.qmp.as_mut() {
            Some(qmp) => qmp.migrate_to_tcp(addr, MIGRATE_TIMEOUT),
            None => Err(SandboxError::NotRunning),
        };
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let received = receiver
            .join()
            .map_err(|_| SandboxError::Relay("relay thread panicked".into()))?;

        migrate_result?;
        let bytes = received?;

        self.emit(
            "sandbox.snapshot.save.real",
            serde_json::json!({
                "name": name,
                "bytes": bytes,
                "mode": "tcp-relay",
            }),
        );
        Ok(())
    }

    /// Start a VM restored from a **real** snapshot taken by
    /// [`Self::save_snapshot_real`].
    ///
    /// The boot command line and the QEMU parameters must match the ones used
    /// when the snapshot was taken; QEMU itself rejects mismatched state.
    pub fn resume_from_snapshot_real(
        config: VMConfig,
        snapshot_path: &Path,
        audit: Arc<Mutex<dyn AuditSink>>,
    ) -> Result<Self, SandboxError> {
        if !snapshot_path.exists() {
            return Err(SandboxError::Snapshot(format!(
                "snapshot not found: {}",
                snapshot_path.display()
            )));
        }
        let mut config = config;
        config.incoming_snapshot = Some(snapshot_path.to_path_buf());

        // `-incoming tcp:` makes **QEMU** listen on a port we picked and released
        // (see `relay::free_local_port`), so another process can steal it in that
        // window; QEMU then fails to bind, or the QMP connection is reset
        // (Windows 10054). Retry with a fresh port instead of failing the restore.
        const RESUME_ATTEMPTS: usize = 3;
        let mut reasons: Vec<String> = Vec::new();
        for attempt in 1..=RESUME_ATTEMPTS {
            let started = Self::new(config.clone(), Arc::clone(&audit))
                .and_then(|mut vm| vm.start().map(|()| vm));
            match started {
                Ok(vm) => return Ok(vm),
                Err(e) => {
                    let reason = e.to_string();
                    if let Ok(mut sink) = audit.lock() {
                        sink.record(AuditEvent::new(
                            AUDIT_ACTOR,
                            "sandbox.snapshot.resume.retry",
                            serde_json::json!({ "attempt": attempt, "reason": reason }),
                        ));
                    }
                    reasons.push(format!("attempt {attempt}: {reason}"));
                    if attempt < RESUME_ATTEMPTS {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
            }
        }
        Err(SandboxError::Relay(format!(
            "failed to resume from snapshot after {RESUME_ATTEMPTS} attempts: {}",
            reasons.join("; ")
        )))
    }

    /// Wait until the guest reports `running` (used after `-incoming`).
    fn wait_for_running(&mut self, timeout: Duration) -> Result<(), SandboxError> {
        let deadline = Instant::now() + timeout;
        loop {
            let status = match self.qmp.as_mut() {
                Some(qmp) => qmp.query_status()?,
                None => return Err(SandboxError::NotRunning),
            };
            match status.as_str() {
                "running" => return Ok(()),
                "shutdown" | "internal-error" => {
                    return Err(SandboxError::Relay(format!(
                        "guest ended up in state '{status}' while restoring"
                    )))
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                return Err(SandboxError::RelayTimeout(format!(
                    "waiting for restored guest to run (last status: {status})"
                )));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
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
        let mut args = vec![
            "-machine".into(),
            VM_MACHINE.into(),
            "-cpu".into(),
            VM_CPU.into(),
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
        ];

        // Restoring: QEMU pulls the migration stream from our relay.
        if let Some(addr) = self.config.incoming_relay_addr {
            args.push("-incoming".into());
            args.push(format!("tcp:{}:{}", addr.ip(), addr.port()));
        }

        args
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

/// Locate the `qemu-system-riscv64` binary (v0.3 #5a).
///
/// Order: `RISCDOM_QEMU` → `QEMU_SYSTEM_RISCV64` → well-known install locations
/// → `PATH`. Falls back to the bare executable name so `start()` can report the
/// full diagnostics when nothing exists.
fn resolve_qemu_binary() -> PathBuf {
    match crate::qemu_discover::discover() {
        Ok(location) => location.exe,
        Err(_) => PathBuf::from(crate::qemu_discover::exe_name()),
    }
}
