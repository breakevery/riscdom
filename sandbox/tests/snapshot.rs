//! Stage 3b test: QMP client + MVP snapshot save/load fallback (audited).

mod common;

use audit::{verify_chain, AuditSink, AuditStore, ChainStatus, SqliteAuditSink};
use common::{build_guest_elf, two_free_ports, wait_for_serial};
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn stage3b_snapshot_save_and_load() {
    let elf = build_guest_elf();

    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("stage3b");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create tmp");
    let snapshot_dir = tmp.join("snapshots");

    let (qmp_port, serial_port) = two_free_ports();

    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("audit store")));
    let sink = SqliteAuditSink::from_shared(Arc::clone(&shared));
    let audit: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(sink));

    let config = VMConfig {
        kernel: elf,
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir: snapshot_dir.clone(),
        serial_observer: None,
    };

    let mut vm = RiscVVirtualMachine::new(config, audit).expect("construct vm");
    vm.start().expect("start vm");
    assert!(
        wait_for_serial(&vm, b"HELLO RISCV", Duration::from_secs(10)),
        "banner (first boot) not captured"
    );

    vm.save_snapshot("s1").expect("save snapshot");
    assert!(
        snapshot_dir.join("s1.json").exists(),
        "snapshot file missing"
    );
    vm.stop().expect("stop vm");

    vm.load_snapshot("s1").expect("load snapshot");
    assert!(
        wait_for_serial(&vm, b"HELLO RISCV", Duration::from_secs(10)),
        "banner (after load) not captured"
    );
    let text = String::from_utf8_lossy(&vm.serial_output()).to_string();
    vm.stop().expect("stop vm");

    assert!(
        text.contains("HELLO RISCV"),
        "boot output after load: {text:?}"
    );

    let store = shared.lock().expect("lock store");
    let status = verify_chain(&store).expect("verify chain");
    assert!(
        matches!(status, ChainStatus::Intact { .. }),
        "chain not intact: {status:?}"
    );
    let actions: Vec<String> = store
        .all()
        .expect("all events")
        .into_iter()
        .map(|e| e.event.action)
        .collect();
    for want in [
        "vm.start",
        "vm.snapshot.save",
        "vm.snapshot.load",
        "vm.stop",
    ] {
        assert!(
            actions.iter().any(|a| a == want),
            "audit missing {want}: {actions:?}"
        );
    }
}
