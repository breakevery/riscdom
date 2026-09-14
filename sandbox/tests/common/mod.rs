//! Shared helpers for the sandbox integration tests.
#![allow(dead_code)]

use sandbox::vm::RiscVVirtualMachine;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// Two distinct free TCP ports, held open until both are read.
pub fn two_free_ports() -> (u16, u16) {
    let a = TcpListener::bind("127.0.0.1:0").expect("bind port a");
    let b = TcpListener::bind("127.0.0.1:0").expect("bind port b");
    (
        a.local_addr().unwrap().port(),
        b.local_addr().unwrap().port(),
    )
}

/// Locate the RISC-V cross compiler.
pub fn riscv_gcc() -> PathBuf {
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

/// Compile the fixture guest into a bare-metal ELF under the target tmp dir.
pub fn build_guest_elf() -> PathBuf {
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

/// Poll `serial_output` until `needle` appears or `timeout` elapses.
pub fn wait_for_serial(vm: &RiscVVirtualMachine, needle: &[u8], timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let out = vm.serial_output();
        if out.windows(needle.len()).any(|w| w == needle) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}
