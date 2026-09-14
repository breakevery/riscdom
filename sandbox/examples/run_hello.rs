//! Minimal end-to-end usage example for the `sandbox` crate.
//!
//! Builds the bundled bare-metal guest (unless a kernel path is given as the
//! first argument), boots it in QEMU, prints the captured UART output, and
//! stops the VM.
//!
//! ```text
//! cargo run -p sandbox --example run_hello
//! cargo run -p sandbox --example run_hello -- path/to/kernel.elf
//! ```

use sandbox::{FileAuditSink, QmpEndpoint, RiscVVirtualMachine, SerialEndpoint, VMConfig};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn two_free_ports() -> (u16, u16) {
    let a = TcpListener::bind("127.0.0.1:0").expect("bind a");
    let b = TcpListener::bind("127.0.0.1:0").expect("bind b");
    (
        a.local_addr().unwrap().port(),
        b.local_addr().unwrap().port(),
    )
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
    let out_dir = std::env::temp_dir().join("riscdom-example");
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

fn main() {
    let kernel = match std::env::args().nth(1) {
        Some(p) => PathBuf::from(p),
        None => build_guest(),
    };

    let work = std::env::temp_dir().join("riscdom-example");
    std::fs::create_dir_all(&work).expect("create work dir");

    let audit = Arc::new(FileAuditSink::new(work.join("audit.jsonl")));
    let (qmp_port, serial_port) = two_free_ports();
    let config = VMConfig {
        kernel: kernel.clone(),
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir: work.join("snapshots"),
    };

    println!("kernel : {}", kernel.display());
    println!("audit  : {}", work.join("audit.jsonl").display());

    let mut vm = RiscVVirtualMachine::new(config, audit).expect("construct vm");
    vm.start().expect("start vm");
    println!("QEMU started (qmp={qmp_port}, serial={serial_port})");

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if String::from_utf8_lossy(&vm.serial_output()).contains("HELLO RISCV") {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let out = String::from_utf8_lossy(&vm.serial_output()).to_string();
    println!("serial : {}", out.trim_end());

    vm.stop().expect("stop vm");
    println!("stopped.");
}
