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

/// How much of QEMU's own output to fold into an error (v1.0 CA-1).
const QEMU_LOG_TAIL_BYTES: usize = 4096;

/// How long a *successful* start waits for the snapshot sender's result to be readable (v1.0
/// CA-1). The migration has completed by then, so this is a courtesy wait, not a verdict.
const SENDER_JOIN_GRACE: Duration = Duration::from_secs(2);

/// QEMU machine the sandbox boots (`-machine`).
///
/// Exported so anything that has to *name* the machine for a run (the host's
/// configuration fingerprint) reads one authority instead of copying the
/// literal — a copy that silently goes stale when this changes (v0.4 1e).
pub const VM_MACHINE: &str = "virt";

/// CPU model the sandbox boots (`-cpu`); exported for the same reason as
/// [`VM_MACHINE`].
pub const VM_CPU: &str = "rv64";

/// Extension of a real snapshot file (a migration stream): `<name>.mig`.
///
/// Exported because the host lists, saves and restores those files, and the file
/// format belongs to the sandbox (v0.4 1e-followup).
pub const SNAPSHOT_MIG_EXT: &str = "mig";

/// Extension of the reboot-fallback snapshot file: `<name>.json`.
pub const SNAPSHOT_JSON_EXT: &str = "json";

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
    /// Where QEMU's own stdout/stderr are captured (v1.0 CA-1). `None` until a start.
    qemu_log: Option<PathBuf>,
    /// Ports this VM reserved for QEMU to bind (v0.4 #1).
    ///
    /// They stay reserved while the VM lives, so a second port request in this
    /// process cannot be handed one of them; the OS-level hold is released in
    /// [`Self::start`] just before QEMU is spawned.
    port_leases: Vec<crate::relay::PortLease>,
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
            qemu_log: None,
            port_leases: Vec::new(),
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
        // Leases from an earlier start are done with.
        self.port_leases.clear();

        // Incoming migration: QEMU *listens* on `-incoming tcp:<addr>`, and a thread of ours connects
        // and pushes the snapshot into it. The port comes from the process-wide lease (v0.4 #1) and
        // stays reserved until this VM is dropped.
        //
        // The **address** has to be in QEMU's arguments, so the lease is taken here — but the sender
        // is **not** started yet (v1.0 CA-1). While our own listener is still bound, a sender that
        // connects would land on *our* socket, and the `hand_off` below would then reset it: the
        // sender would die before QEMU ever saw a stream, and the first sign of it would be QEMU's
        // QMP socket dying. It starts below, after the hand-off and the spawn.
        let mut incoming: Option<(PathBuf, std::net::SocketAddr)> = None;
        if let Some(snapshot) = self.config.incoming_snapshot.clone() {
            let lease = crate::relay::lease_local_port()?;
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], lease.port()));
            self.config.incoming_relay_addr = Some(addr);
            self.port_leases.push(lease);
            incoming = Some((snapshot, addr));
        }

        let args = self.qemu_args();
        // QEMU's own words are **captured** rather than discarded (v1.0 CA-1): when a start fails,
        // the reason is usually in QEMU's stderr, and a null stream threw it away. One file per VM,
        // in the platform temp directory, removed with the VM.
        let log_path = qemu_log_path();
        let log_file = std::fs::File::create(&log_path)
            .map_err(|e| SandboxError::Io(format!("qemu log {}: {e}", log_path.display())))?;
        let log_stdout = log_file
            .try_clone()
            .map_err(|e| SandboxError::Io(e.to_string()))?;
        self.qemu_log = Some(log_path);

        let mut command = Command::new(&self.qemu_bin);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_stdout))
            .stderr(Stdio::from(log_file));

        // Windows only (v0.5 batch 12, the walkthrough's G-4). QEMU for Windows is a
        // console-subsystem program, so a child started the ordinary way opens a
        // console window of its own — a stray window that appears over the app and
        // takes focus. `-display none` (see `qemu_args`) hides the *guest* display and
        // does nothing about that console. CREATE_NO_WINDOW gives the child a
        // console-less creation; stdio is already redirected, and nothing else changes.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        // Last moment: let go of the listeners so QEMU can bind the ports. The
        // numbers stay reserved here until this VM is dropped.
        for lease in &mut self.port_leases {
            lease.hand_off();
        }

        let child = command
            .spawn()
            .map_err(|e| SandboxError::Spawn(format!("{} ({e})", self.qemu_bin.display())))?;
        self.child = Some(child);

        // The lease is handed off and QEMU is starting, so the sender can no longer land on our own
        // listener (v1.0 CA-1). `send_file_to` retries its connect until QEMU binds the port.
        if let Some((snapshot, addr)) = incoming {
            self.snapshot_sender = Some(std::thread::spawn(move || {
                crate::relay::send_file_to(addr, &snapshot, crate::relay::DEFAULT_RELAY_TIMEOUT)
            }));
        }

        // Everything after the spawn runs in one place so a failure can fold in what the snapshot
        // sender saw (v1.0 CA-1): a sender that died is a fact about *this* start, not a detail for
        // whoever later wonders why QEMU's QMP socket died.
        let started = self.finish_start();
        let sender = self.take_snapshot_sender(started.is_ok());
        match (started, sender) {
            (Ok(()), Some(Err(error))) => {
                return Err(SandboxError::Relay(format!(
                    "the snapshot sender failed while restoring: {error}"
                )))
            }
            (Ok(()), _) => {}
            (Err(error), Some(Err(sender_error))) => {
                return Err(SandboxError::Relay(format!(
                    "{error}; the snapshot sender also failed: {sender_error}"
                )))
            }
            (Err(error), _) => return Err(error),
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

    /// The rest of a start: attach the serial reader, negotiate QMP, and (with `-incoming`) wait for
    /// the restored guest to run (v1.0 CA-1 — split out of [`Self::start`] so its failure and the
    /// snapshot sender's can be reported together).
    fn finish_start(&mut self) -> Result<(), SandboxError> {
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
                                if let Err(error) = sink.record(AuditEvent::new(
                                    AUDIT_ACTOR,
                                    "serial.read",
                                    serde_json::json!({ "bytes": n }),
                                )) {
                                    audit::report_failure(&error);
                                }
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
                                        if let Err(error) = sink.record(AuditEvent::new(
                                            AUDIT_ACTOR,
                                            "sandbox.serial.observer_panic",
                                            serde_json::json!({ "bytes": n }),
                                        )) {
                                            audit::report_failure(&error);
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            }));
        }

        // 2. QMP: connect, consume greeting, negotiate capabilities. A QEMU that has already
        //    given up — a handed-off port taken by somebody else, say — is named here rather than
        //    left as a bare connect failure (v1.0 batch AN).
        let connect = QmpClient::connect(&self.config.qmp, CONNECT_TIMEOUT);
        let qmp = connect.map_err(|e| self.explain_qmp("the QMP connect", e))?;
        self.qmp = Some(qmp);

        // 3. With `-incoming`, wait until the restored guest is actually running.
        if self.config.incoming_snapshot.is_some() {
            self.wait_for_running(MIGRATE_TIMEOUT)?;
        }

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
        // The sender is done with by now, or was never readable; its result is no longer news.
        let _ = self.take_snapshot_sender(false);
        self.forget_qemu_log();

        if was_running {
            self.emit("vm.stop", serde_json::json!({}));
        }
        Ok(())
    }

    /// Remove this VM's QEMU-output capture, best effort (v1.0 CA-1).
    ///
    /// The log is for a failure that is happening; a failure that has been reported carries its
    /// tail in the error already, and a closed VM has nothing more to say.
    fn forget_qemu_log(&mut self) {
        if let Some(path) = self.qemu_log.take() {
            let _ = std::fs::remove_file(path);
        }
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
        let path = self
            .config
            .snapshot_dir
            .join(format!("{name}.{SNAPSHOT_MIG_EXT}"));
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

        let migrate_result =
            self.qmp_op("migrate", |qmp| qmp.migrate_to_tcp(addr, MIGRATE_TIMEOUT));
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

        // `-incoming tcp:` makes **QEMU** listen on a port that this process no
        // longer holds (see `relay::PortLease`), so another process can still
        // steal it in the window before QEMU binds; QEMU then fails to bind, or
        // the QMP connection is reset (Windows 10054). Retry with a fresh port
        // instead of failing the restore — the lease narrows the window, the
        // retry covers what is left.
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
                        if let Err(error) = sink.record(AuditEvent::new(
                            AUDIT_ACTOR,
                            "sandbox.snapshot.resume.retry",
                            serde_json::json!({ "attempt": attempt, "reason": reason }),
                        )) {
                            audit::report_failure(&error);
                        }
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
            let status = self.qmp_op("query-status", QmpClient::query_status)?;
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

    /// Wait a moment for the snapshot sender's result, and answer it (v1.0 CA-1).
    ///
    /// `block` is true on the success path only: there the migration has completed, so the sender
    /// has finished or is about to, and waiting reads its verdict. On an error path the sender may
    /// still be inside its 60 s connect timeout, and a failing start must not wait that out — the
    /// handle is dropped (detaching the thread) and `None` says the result was not readable.
    fn take_snapshot_sender(&mut self, block: bool) -> Option<Result<u64, SandboxError>> {
        let handle = self.snapshot_sender.take()?;
        if block {
            let deadline = Instant::now() + SENDER_JOIN_GRACE;
            while !handle.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        if !handle.is_finished() {
            return None;
        }
        Some(match handle.join() {
            Ok(outcome) => outcome,
            Err(_) => Err(SandboxError::Relay("the snapshot sender panicked".into())),
        })
    }

    /// The tail of QEMU's own output, when there is any (v1.0 CA-1).
    fn qemu_log_tail(&self, max: usize) -> Option<String> {
        tail_of(self.qemu_log.as_deref()?, max)
    }

    /// Run one QMP operation, reconnecting once when the socket died but QEMU lives (v1.0 CA-1).
    ///
    /// This is the depth behind the root fix: a socket that is reset while its process is still
    /// there is a *transport* failure, and one fresh connection answers it. When the process is
    /// gone the operation's own error is returned, and `explain_qmp` names the exit.
    fn qmp_op<T>(
        &mut self,
        what: &str,
        op: impl Fn(&mut QmpClient) -> Result<T, SandboxError>,
    ) -> Result<T, SandboxError> {
        let endpoint = self.config.qmp.clone();
        let mut client = self.qmp.take();
        let outcome = run_qmp_with_one_reconnect(&endpoint, &mut client, || self.is_running(), op);
        self.qmp = client;
        outcome.map_err(|error| self.explain_qmp(what, error))
    }

    /// Explain a failed QMP operation (v1.0 batch AN).
    ///
    /// A QMP socket that dies mid-operation reports `os error 10054` on Windows — or a bare
    /// connection reset — which says what the *socket* did and nothing about *why*. The usual why
    /// is that QEMU is gone: it could not bind a port we handed it (`relay::PortLease` names that
    /// window) or the guest died. So the child's own exit is checked here and named. If QEMU is
    /// **still running**, the original error is kept: a socket that died while its process lives is
    /// a different fact, and inventing an exit for it would be a lie.
    fn explain_qmp(&mut self, what: &str, error: SandboxError) -> SandboxError {
        let exited = match self.child.as_mut() {
            Some(child) => match child.try_wait() {
                Ok(Some(status)) => Some(status),
                _ => None,
            },
            None => None,
        };
        match exited {
            Some(status) => {
                let explained = qmp_failure(what, status, &error);
                // QEMU's own last words, when the capture caught any (v1.0 CA-1).
                match self.qemu_log_tail(QEMU_LOG_TAIL_BYTES) {
                    Some(tail) => {
                        SandboxError::Qmp(format!("{explained}; QEMU's last output was: {tail}"))
                    }
                    None => explained,
                }
            }
            None => error,
        }
    }

    fn snapshot_path(&self, name: &str) -> PathBuf {
        self.config
            .snapshot_dir
            .join(format!("{name}.{SNAPSHOT_JSON_EXT}"))
    }

    /// Record an audit event (a failed write is reported, never silent — v0.8).
    fn emit(&self, action: &str, detail: serde_json::Value) {
        if let Ok(mut sink) = self.audit.lock() {
            if let Err(error) = sink.record(AuditEvent::new(AUDIT_ACTOR, action, detail)) {
                audit::report_failure(&error);
            }
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
        self.forget_qemu_log();
    }
}

/// Where one VM's QEMU output is captured (v1.0 CA-1): a per-process, per-VM temp file.
///
/// The number comes from a process-wide sequence: `VMConfig` carries no id, and two VMs of one
/// process (a restore's retries do exactly this) must not share a log.
fn qemu_log_path() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("riscdom-qemu-{}-{n}.log", std::process::id()))
}

/// The last `max` bytes of a file as text, when it has any (v1.0 CA-1).
///
/// `None` for a missing or empty file: an absent capture must read as "nothing to say", not as an
/// empty line in an error.
fn tail_of(path: &Path, max: usize) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let start = bytes.len().saturating_sub(max);
    Some(String::from_utf8_lossy(&bytes[start..]).trim().to_string())
}

/// Run a QMP operation, and once more after a reconnect when `qemu_alive()` says the process is
/// still there (v1.0 CA-1).
///
/// A socket reset with the process alive is a transport failure rather than a verdict: the client
/// is rebuilt from `endpoint` and the operation is tried again. When the process is **gone** the
/// original error is returned untouched, because its caller has the exit to name.
fn run_qmp_with_one_reconnect<T>(
    endpoint: &QmpEndpoint,
    client: &mut Option<QmpClient>,
    mut qemu_alive: impl FnMut() -> bool,
    op: impl Fn(&mut QmpClient) -> Result<T, SandboxError>,
) -> Result<T, SandboxError> {
    let first = match client.as_mut() {
        Some(open) => op(open),
        None => return Err(SandboxError::NotRunning),
    };
    let error = match first {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    if !qemu_alive() {
        return Err(error);
    }
    *client = Some(QmpClient::connect(endpoint, CONNECT_TIMEOUT)?);
    let retried = match client.as_mut() {
        Some(open) => op(open),
        None => return Err(SandboxError::NotRunning),
    };
    retried.map_err(|second| {
        SandboxError::Qmp(format!(
            "the connection was reset and the retry failed too: {second} \
             (the first error was: {error})"
        ))
    })
}

/// The message for a QMP failure whose QEMU has exited (v1.0 batch AN).
///
/// Split out so a test can check the wording without a guest: what it reports is the exit, and the
/// original socket error is kept beside it rather than thrown away.
fn qmp_failure(what: &str, status: std::process::ExitStatus, error: &SandboxError) -> SandboxError {
    let how = match status.code() {
        Some(code) => format!("code {code}"),
        None => "no exit code (killed by a signal)".to_string(),
    };
    SandboxError::Qmp(format!(
        "QEMU exited with {how} during {what} (the original error was: {error})"
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A QMP failure whose QEMU is gone names the exit **and** keeps the socket error
    /// (v1.0 batch AN).
    #[test]
    fn a_qmp_failure_names_the_exit_of_a_dead_qemu() {
        let status = exited_with(3);
        let error = qmp_failure(
            "query-status",
            status,
            &SandboxError::Qmp("os error 10054".to_string()),
        );
        let text = error.to_string();
        assert!(text.contains("QEMU exited with code 3"), "{text}");
        assert!(text.contains("during query-status"), "{text}");
        assert!(
            text.contains("os error 10054"),
            "the socket error is kept: {text}"
        );
    }

    /// The tail helper reads back what a capture holds, and answers nothing for a missing or empty
    /// one (v1.0 CA-1).
    #[test]
    fn the_qemu_log_tail_is_read_back_and_bounded() {
        let dir = std::env::temp_dir().join(format!("riscdom-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("qemu.log");
        assert_eq!(tail_of(&path, 64), None, "a missing file has no tail");
        std::fs::write(&path, b"").unwrap();
        assert_eq!(tail_of(&path, 64), None, "an empty file has no tail");
        std::fs::write(&path, b"qemu: could not bind socket\n").unwrap();
        let tail = tail_of(&path, 64).expect("a tail");
        assert!(tail.contains("could not bind socket"), "{tail}");
        std::fs::write(&path, vec![b'x'; 100]).unwrap();
        assert_eq!(tail_of(&path, 10).expect("a tail").len(), 10, "bounded");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sender that died is read back as the error it was (v1.0 CA-1); before this batch the
    /// handle was joined and the result thrown away.
    #[test]
    fn a_dead_snapshot_sender_is_reported() {
        let mut vm = fake_vm();
        vm.snapshot_sender = Some(std::thread::spawn(|| {
            Err(SandboxError::Relay("connection reset by peer".into()))
        }));
        let outcome = vm.take_snapshot_sender(true).expect("a readable result");
        let error = outcome.expect_err("the sender failed");
        assert!(error.to_string().contains("connection reset"), "{error}");
    }

    /// A sender that is still trying is not waited for on a failing start (v1.0 CA-1): its result
    /// is simply not readable, and the thread is left to end on its own.
    #[test]
    fn a_sender_still_trying_is_not_waited_for() {
        let mut vm = fake_vm();
        vm.snapshot_sender = Some(std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(0)
        }));
        assert!(vm.take_snapshot_sender(false).is_none(), "not readable yet");
    }

    /// A reset QMP socket with QEMU alive is reconnected once, and the operation is retried
    /// (v1.0 CA-1). Uses a socket that speaks just enough QMP, and no QEMU at all.
    #[test]
    fn a_reset_qmp_socket_is_reconnected_once() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let server = std::thread::spawn(move || {
            // First connection: greet, negotiate, then hang up without answering.
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let _ = speak_qmp(&mut stream, &mut reader, false);
            }
            // Second connection: greet, negotiate, and answer.
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = std::io::BufReader::new(stream.try_clone().expect("clone"));
                let _ = speak_qmp(&mut stream, &mut reader, true);
            }
        });
        let endpoint = QmpEndpoint::tcp("127.0.0.1", port);
        let mut client = Some(QmpClient::connect(&endpoint, CONNECT_TIMEOUT).expect("connect"));
        let outcome =
            run_qmp_with_one_reconnect(&endpoint, &mut client, || true, QmpClient::query_status);
        assert_eq!(outcome.expect("reconnected and retried"), "running");
        let _ = server.join();
    }

    /// Just enough QMP: the greeting, the `qmp_capabilities` reply, and — when `answer` — a reply to
    /// the one command that follows. With `answer` false the connection simply ends, which is what a
    /// reset looks like to the client.
    fn speak_qmp(
        stream: &mut std::net::TcpStream,
        reader: &mut std::io::BufReader<std::net::TcpStream>,
        answer: bool,
    ) -> std::io::Result<()> {
        use std::io::{BufRead, Write};
        stream.write_all(b"{\"QMP\":{\"version\":{}}}\r\n")?;
        stream.flush()?;
        let mut line = String::new();
        reader.read_line(&mut line)?;
        stream.write_all(b"{\"return\":{}}\r\n")?;
        stream.flush()?;
        line.clear();
        reader.read_line(&mut line)?;
        if answer {
            stream.write_all(b"{\"return\":{\"status\":\"running\"}}\r\n")?;
            stream.flush()?;
        }
        Ok(())
    }

    /// A VM handle that owns no process — enough to drive the helpers that read its own state.
    fn fake_vm() -> RiscVVirtualMachine {
        let dir = std::env::temp_dir().join(format!("riscdom-vm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let kernel = dir.join("kernel.elf");
        std::fs::write(&kernel, b"not really an ELF").unwrap();
        let config = VMConfig {
            kernel,
            memory_mb: 64,
            qmp: QmpEndpoint::tcp("127.0.0.1", 1),
            serial: SerialEndpoint::file(dir.join("serial.log")),
            snapshot_dir: dir,
            serial_observer: None,
            incoming_snapshot: None,
            incoming_relay_addr: None,
            qemu_exe: Some(std::env::current_exe().expect("current exe")),
        };
        let audit: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(audit::SqliteAuditSink::new(
            audit::AuditStore::in_memory().expect("store"),
        )));
        RiscVVirtualMachine::new(config, audit).expect("vm")
    }

    /// A real child that exits `code` on its own, so the `ExitStatus` is the platform's own and
    /// not a value this test invented.
    fn exited_with(code: i32) -> std::process::ExitStatus {
        #[cfg(windows)]
        let child = {
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", &format!("exit {code}")]);
            command.spawn()
        };
        #[cfg(not(windows))]
        let child = {
            let mut command = std::process::Command::new("sh");
            command.args(["-c", &format!("exit {code}")]);
            command.spawn()
        };
        child.expect("spawn the stand-in").wait().expect("wait")
    }
}
