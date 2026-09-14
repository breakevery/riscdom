//! Stage 3b test: QMP client + MVP snapshot save/load fallback.

mod common;

use common::{build_guest_elf, two_free_ports, wait_for_serial};
use sandbox::audit_sink::FileAuditSink;
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn stage3b_snapshot_save_and_load() {
    let elf = build_guest_elf();

    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("stage3b");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    let audit_path = tmp.join("audit.jsonl");
    let snapshot_dir = tmp.join("snapshots");

    let (qmp_port, serial_port) = two_free_ports();

    let audit = Arc::new(FileAuditSink::new(audit_path.clone()));
    let config = VMConfig {
        kernel: elf,
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir: snapshot_dir.clone(),
    };

    let mut vm = RiscVVirtualMachine::new(config, audit.clone()).expect("construct vm");
    vm.start().expect("start vm");
    assert!(
        wait_for_serial(&vm, b"HELLO RISCV", Duration::from_secs(10)),
        "banner (first boot) not captured"
    );

    // Save a snapshot, then stop.
    vm.save_snapshot("s1").expect("save snapshot");
    assert!(snapshot_dir.join("s1.json").exists(), "snapshot file missing");
    vm.stop().expect("stop vm");

    // Restore it (stop + reboot with the stored parameters).
    vm.load_snapshot("s1").expect("load snapshot");
    assert!(
        wait_for_serial(&vm, b"HELLO RISCV", Duration::from_secs(10)),
        "banner (after load) not captured: {:?}",
        String::from_utf8_lossy(&vm.serial_output())
    );
    let text = String::from_utf8_lossy(&vm.serial_output()).to_string();
    vm.stop().expect("stop vm");

    assert!(text.contains("HELLO RISCV"), "boot output after load: {text:?}");

    let log = std::fs::read_to_string(&audit_path).expect("read audit log");
    for action in [
        "vm.start",
        "vm.snapshot.save",
        "vm.snapshot.load",
        "vm.stop",
    ] {
        assert!(log.contains(action), "audit log missing {action}: \n{log}");
    }
}
