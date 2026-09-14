//! Stage 3a smoke test: QEMU lifecycle + serial capture + audit events.
//!
//! Requires `qemu-system-riscv64` and `riscv64-unknown-elf-gcc` to be
//! available (see repo `ENVIRONMENT.md`). Override with `RISCDOM_QEMU` and
//! `RISCDOM_RISCV_GCC` if they are not on `PATH`.

use sandbox::audit_sink::FileAuditSink;
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Two distinct free TCP ports, held open until both are read.
fn two_free_ports() -> (u16, u16) {
    let a = TcpListener::bind("127.0.0.1:0").expect("bind port a");
    let b = TcpListener::bind("127.0.0.1:0").expect("bind port b");
    (
        a.local_addr().unwrap().port(),
        b.local_addr().unwrap().port(),
    )
}

/// Locate the RISC-V cross compiler.
fn riscv_gcc() -> PathBuf {
    if let Ok(p) = std::env::var("RISCDOM_RISCV_GCC") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if cfg!(windows) {
        let fallback = PathBuf::from(r"D:\tools\riscv64-unknown-elf\bin\riscv64-unknown-elf-gcc.exe");
        if fallback.exists() {
            return fallback;
        }
        PathBuf::from("riscv64-unknown-elf-gcc.exe")
    } else {
        PathBuf::from("riscv64-unknown-elf-gcc")
    }
}

/// Compile the fixture guest into a bare-metal ELF under the target tmp dir.
fn build_guest_elf() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures = manifest.join("tests").join("fixtures");
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("guest");
    std::fs::create_dir_all(&out_dir).expect("create guest dir");
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
        .expect("failed to run riscv64-unknown-elf-gcc");
    assert!(status.success(), "riscv gcc failed to build guest ELF");
    elf
}

#[test]
fn stage3a_lifecycle_serial_and_audit() {
    let elf = build_guest_elf();

    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("stage3a");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    let audit_path = tmp.join("audit.jsonl");
    let snapshot_dir = tmp.join("snapshots");
    std::fs::create_dir_all(&snapshot_dir).expect("create snapshots");

    let (qmp_port, serial_port) = two_free_ports();

    let audit = Arc::new(FileAuditSink::new(audit_path.clone()));
    let config = VMConfig {
        kernel: elf,
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir,
    };

    let mut vm = RiscVVirtualMachine::new(config, audit.clone()).expect("construct vm");
    vm.start().expect("start vm");

    // Wait (<=10s) for the guest banner on the UART.
    let needle = b"HELLO RISCV";
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let out = vm.serial_output();
        if out.windows(needle.len()).any(|w| w == needle) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let captured = vm.serial_output();
    vm.stop().expect("stop vm");

    let text = String::from_utf8_lossy(&captured);
    assert!(
        text.contains("HELLO RISCV"),
        "expected guest banner, got: {text:?}"
    );

    // Audit coverage: vm.start, serial.read, vm.stop must all be present.
    let log = std::fs::read_to_string(&audit_path).expect("read audit log");
    for action in ["vm.start", "serial.read", "vm.stop"] {
        assert!(log.contains(action), "audit log missing {action}: \n{log}");
    }
}
