//! Stage 15a — the serial observer callback.

mod common;

use audit::{AuditSink, AuditStore, SqliteAuditSink};
use common::{build_guest, two_free_ports, wait_for_serial};
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, SerialObserver, VMConfig};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn sink() -> (Arc<Mutex<dyn AuditSink>>, Arc<Mutex<AuditStore>>) {
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("store")));
    let s: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(SqliteAuditSink::from_shared(
        Arc::clone(&shared),
    )));
    (s, shared)
}

fn config(elf: PathBuf, observer: Option<SerialObserver>) -> VMConfig {
    let (qmp_port, serial_port) = two_free_ports();
    VMConfig {
        kernel: elf,
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir: std::env::temp_dir().join("riscdom-observer-snapshots"),
        serial_observer: observer,
        incoming_snapshot: None,
        incoming_relay_addr: None,
        qemu_exe: None,
    }
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn observer_receives_the_same_bytes_as_the_buffer() {
    let elf = build_guest("hello_split.c");
    let (audit, _shared) = sink();

    let collected: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicUsize::new(0));

    let sink_bytes = Arc::clone(&collected);
    let sink_calls = Arc::clone(&calls);
    let observer: SerialObserver = Arc::new(move |chunk: &[u8]| {
        sink_calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut guard) = sink_bytes.lock() {
            guard.extend_from_slice(chunk);
        }
    });

    let mut vm =
        RiscVVirtualMachine::new(config(elf, Some(observer)), audit).expect("construct vm");
    vm.start().expect("start vm");

    assert!(
        wait_for_serial(&vm, b"HELLO RISCV", Duration::from_secs(10)),
        "banner not captured: {:?}",
        String::from_utf8_lossy(&vm.serial_output())
    );
    vm.stop().expect("stop vm");

    let observed = collected.lock().expect("lock").clone();
    let buffered = vm.serial_output();

    // Observer data must match the buffer exactly.
    assert_eq!(observed, buffered, "observer bytes != serial_output()");
    assert!(
        String::from_utf8_lossy(&observed).contains("HELLO RISCV"),
        "observer missed the banner"
    );
    // The guest prints in two separated bursts -> at least two frames.
    assert!(
        calls.load(Ordering::SeqCst) >= 2,
        "expected >= 2 observer calls, got {}",
        calls.load(Ordering::SeqCst)
    );
}

#[test]
#[ignore = "requires a QEMU guest and a RISC-V GCC; run with --include-ignored"]
fn panicking_observer_does_not_kill_the_vm() {
    let elf = build_guest("hello_split.c");
    let (audit, shared) = sink();

    let observer: SerialObserver = Arc::new(|_chunk: &[u8]| {
        panic!("observer intentionally panics");
    });

    let mut vm =
        RiscVVirtualMachine::new(config(elf, Some(observer)), audit).expect("construct vm");
    vm.start().expect("start vm");

    // Output still reaches the buffer even though the observer panics.
    assert!(
        wait_for_serial(&vm, b"HELLO", Duration::from_secs(10)),
        "buffering broke when the observer panicked"
    );

    // The VM must still stop cleanly.
    vm.stop().expect("stop vm after observer panic");

    // The panic must have been audited.
    let store = shared.lock().expect("lock store");
    let actions: Vec<String> = store
        .all()
        .expect("all")
        .into_iter()
        .map(|e| e.event.action)
        .collect();
    assert!(
        actions.iter().any(|a| a == "sandbox.serial.observer_panic"),
        "missing observer_panic audit event: {actions:?}"
    );
}
