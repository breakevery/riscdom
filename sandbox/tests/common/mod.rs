//! Shared helpers for the sandbox integration tests.
#![allow(dead_code)]

use sandbox::vm::RiscVVirtualMachine;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// Two distinct free TCP ports, reserved by this process for the rest of the run.
///
/// The process-wide guard is now the sandbox's own port lease (v0.4 #1), so the
/// numbers cannot be handed to two threads in this binary; the leases are parked
/// here, handed off to the OS so QEMU can bind, and stay reserved until the
/// process ends.
pub fn two_free_ports() -> (u16, u16) {
    use std::sync::Mutex;
    static HELD: Mutex<Vec<sandbox::relay::PortLease>> = Mutex::new(Vec::new());

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
    build_guest("hello.c")
}

/// Compile a named fixture (e.g. `hello.c`, `hello_split.c`) into an ELF.
///
/// Every call gets its **own** output path. Two parallel tests in one binary
/// used to compile the same fixture into the same file, so a reader could open it
/// while `gcc` was rewriting it — the gate caught that as
/// `kernel not found: …\target\tmp\guest\hello_split.elf` (v0.4 1f).
pub fn build_guest(fixture: &str) -> PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);

    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixtures = manifest.join("tests").join("fixtures");
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("guest");
    std::fs::create_dir_all(&out_dir).expect("create guest dir");
    let stem = fixture.trim_end_matches(".c");
    let elf = out_dir.join(format!(
        "{stem}-{}-{}.elf",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));

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
        .arg(fixtures.join(fixture))
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
