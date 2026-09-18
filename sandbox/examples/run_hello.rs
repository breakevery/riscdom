//! Minimal end-to-end usage example for the `sandbox` crate.
//!
//! Builds the bundled bare-metal guest (unless a kernel path is given as the
//! first argument), boots it in QEMU, prints the captured UART output, stops
//! the VM, and verifies the audit chain written to a SQLite file.
//!
//! ```text
//! cargo run -p sandbox --example run_hello
//! cargo run -p sandbox --example run_hello -- path/to/kernel.elf
//! ```

use audit::{verify_chain, AuditSink, AuditStore, SqliteAuditSink};
use sandbox::{QmpEndpoint, RiscVVirtualMachine, SerialEndpoint, VMConfig};
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Ports this run reserved, kept here so their numbers stay reserved after the
/// OS-level hold is handed off to QEMU (v0.4 #1).
static HELD: Mutex<Vec<sandbox::relay::PortLease>> = Mutex::new(Vec::new());

fn two_free_ports() -> (u16, u16) {
    let mut leases = sandbox::relay::lease_local_ports(2).expect("lease two ports");
    let mut serial = leases.pop().expect("two leases were requested");
    let mut qmp = leases.pop().expect("two leases were requested");
    let ports = (qmp.port(), serial.port());
    qmp.hand_off();
    serial.hand_off();
    let mut held = HELD.lock().expect("port guard");
    held.push(qmp);
    held.push(serial);
    ports
}

fn riscv_gcc() -> PathBuf {
    if let Ok(p) = std::env::var("RISCDOM_RISCV_GCC") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if cfg!(windows) {
        let fallback =
            PathBuf::from(r"D:\tools\riscv64-unknown-elf\bin\riscv64-unknown-elf-gcc.exe");
        if fallback.exists() {
            return fallback;
        }
        PathBuf::from("riscv64-unknown-elf-gcc.exe")
    } else {
        PathBuf::from("riscv64-unknown-elf-gcc")
    }
}

fn build_guest() -> PathBuf {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures");
    let out_dir = std::env::temp_dir().join(format!("riscdom-example-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).expect("create out dir");
    let elf = out_dir.join("hello.elf");
    let status = Command::new(riscv_gcc())
        .args([
            "-march=rv64gc",
            "-mabi=lp64d",
            "-mcmodel=medany",
            "-ffreestanding",
            "-nostdlib",
            "-nostartfiles",
            "-T",
        ])
        .arg(fixtures.join("link.ld"))
        .arg("-o")
        .arg(&elf)
        .arg(fixtures.join("hello.c"))
        .status()
        .expect("run riscv64-unknown-elf-gcc");
    assert!(status.success(), "failed to build guest ELF");
    elf
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let kernel = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => build_guest(),
    };

    let work = std::env::temp_dir().join(format!("riscdom-example-{}", std::process::id()));
    std::fs::create_dir_all(&work)?;
    let db_path = work.join("audit.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(work.join(format!("audit.db{suffix}")));
    }

    let audit: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::new(
        AuditStore::open(&db_path)?,
    )));
    let (qmp_port, serial_port) = two_free_ports();
    let config = VMConfig {
        kernel: kernel.clone(),
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir: work.join("snapshots"),
        serial_observer: None,
        incoming_snapshot: None,
        incoming_relay_addr: None,
        qemu_exe: None,
    };

    println!("kernel : {}", kernel.display());
    println!("audit db: {}", db_path.display());

    let mut vm = RiscVVirtualMachine::new(config, audit)?;
    vm.start()?;
    println!("QEMU started (qmp={qmp_port}, serial={serial_port})");

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if String::from_utf8_lossy(&vm.serial_output()).contains("HELLO RISCV") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    println!(
        "serial : {}",
        String::from_utf8_lossy(&vm.serial_output()).trim_end()
    );
    vm.stop()?;
    drop(vm);

    // Verify the chain from a fresh connection to the on-disk DB.
    let store = AuditStore::open(&db_path)?;
    println!("events : {}", store.count()?);
    println!("verify : {:?}", verify_chain(&store)?);
    Ok(())
}
