//! Stage 19b — real snapshots: QMP `migrate` over a local TCP relay.
//!
//! Requires QEMU + `riscv64-unknown-elf-gcc` on PATH.

mod common;

use audit::{verify_chain, AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use common::{build_guest, two_free_ports, wait_for_serial};
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-snapreal-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create dir");
    dir
}

fn sink() -> (Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let s: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::from_shared(
        Arc::clone(&shared),
    )));
    (s, shared)
}

fn config(elf: PathBuf, snapshot_dir: PathBuf, tag: &str) -> VMConfig {
    let _ = tag;
    let (qmp_port, serial_port) = two_free_ports();
    VMConfig {
        kernel: elf,
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir,
        serial_observer: None,
        incoming_snapshot: None,
        incoming_relay_addr: None,
        qemu_exe: None,
    }
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn real_snapshot_round_trip_via_tcp_relay() {
    let elf = build_guest("hello_phases.c");
    let dir = unique_dir("roundtrip");
    let snapshot_dir = dir.join("snapshots");
    let (audit, shared) = sink();

    // ---- take the snapshot ----
    let mut vm = RiscVVirtualMachine::new(
        config(elf.clone(), snapshot_dir.clone(), "src"),
        audit.clone(),
    )
    .expect("construct vm");
    vm.start().expect("start vm");
    assert!(
        wait_for_serial(&vm, b"PHASE1", Duration::from_secs(10)),
        "guest did not reach PHASE1: {:?}",
        String::from_utf8_lossy(&vm.serial_output())
    );

    vm.save_snapshot_real("s1").expect("save real snapshot");
    let snap = snapshot_dir.join("s1.mig");
    assert!(snap.exists(), "snapshot file missing");
    let size = std::fs::metadata(&snap).expect("metadata").len();
    assert!(size > 0, "snapshot is empty");
    println!("snapshot: {} bytes", size);

    vm.stop().expect("stop source vm");

    // Overwriting is refused.
    let mut vm_again = RiscVVirtualMachine::new(
        config(elf.clone(), snapshot_dir.clone(), "src2"),
        audit.clone(),
    )
    .expect("construct vm");
    vm_again.start().expect("start vm");
    let err = vm_again
        .save_snapshot_real("s1")
        .expect_err("must refuse to overwrite");
    println!("overwrite refused: {err}");
    vm_again.stop().expect("stop vm");

    // ---- restore it ----
    let cfg = config(elf, snapshot_dir.clone(), "dst");
    let mut restored = RiscVVirtualMachine::resume_from_snapshot_real(cfg, &snap, audit.clone())
        .expect("resume from snapshot");

    // The restored guest continues from the migrated point and prints PHASE2.
    assert!(
        wait_for_serial(&restored, b"PHASE2", Duration::from_secs(20)),
        "restored guest did not continue: {:?}",
        String::from_utf8_lossy(&restored.serial_output())
    );
    println!(
        "restored serial: {:?}",
        String::from_utf8_lossy(&restored.serial_output())
    );
    restored.stop().expect("stop restored vm");

    // ---- audit ----
    let store = shared.lock().expect("lock");
    let actions: Vec<String> = store
        .all()
        .expect("all")
        .into_iter()
        .map(|e| e.event.action)
        .collect();
    assert!(
        actions.iter().any(|a| a == "sandbox.snapshot.save.real"),
        "missing real-snapshot audit event: {actions:?}"
    );
    assert!(matches!(
        verify_chain(&store).expect("verify"),
        ChainStatus::Intact { .. }
    ));
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn resuming_a_missing_snapshot_fails_clearly() {
    let elf = build_guest("hello_phases.c");
    let dir = unique_dir("missing");
    let (audit, _shared) = sink();
    let cfg = config(elf, dir.join("snapshots"), "missing");

    let err =
        match RiscVVirtualMachine::resume_from_snapshot_real(cfg, &dir.join("nope.mig"), audit) {
            Ok(_) => panic!("resuming a missing snapshot must fail"),
            Err(e) => e,
        };
    assert!(
        err.to_string().contains("snapshot not found"),
        "unexpected error: {err}"
    );
}
