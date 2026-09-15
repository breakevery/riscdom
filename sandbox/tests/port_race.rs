//! Stage v0.3-4e — port allocation under concurrency (stress test).
//!
//! Boots several QEMU guests at once, repeatedly, and requires every one of them
//! to come up and see its banner. This is the scenario that used to flake in the
//! gate: `two_free_ports()` binds a port, releases it, and QEMU binds it again a
//! moment later — another test process can steal it in that window.
//!
//! Ignored by default (it is slow); run it by hand:
//!
//! ```text
//! cargo test -p sandbox --test port_race -- --ignored --nocapture
//! ```

mod common;

use audit::{AuditSink, AuditStore, SqliteAuditSink};
use common::{build_guest_elf, two_free_ports, wait_for_serial};
use sandbox::platform::{QmpEndpoint, SerialEndpoint};
use sandbox::vm::{RiscVVirtualMachine, VMConfig};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Concurrent guests per round, and how many rounds to run.
const GUESTS: usize = 4;
const ROUNDS: usize = 3;

fn boot_one(tag: usize, elf: &Path, round: usize, failures: &Mutex<Vec<String>>) {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("port_race")
        .join(format!("{round}-{tag}"));
    let _ = std::fs::remove_dir_all(&tmp);
    if std::fs::create_dir_all(tmp.join("snapshots")).is_err() {
        failures
            .lock()
            .unwrap()
            .push(format!("round {round} guest {tag}: cannot create work dir"));
        return;
    }

    // One attempt, no retry: the point is to observe raw port behaviour.
    let (qmp_port, serial_port) = two_free_ports();
    let shared = Arc::new(Mutex::new(AuditStore::in_memory().expect("audit store")));
    let sink = SqliteAuditSink::from_shared(Arc::clone(&shared));
    let audit: Arc<Mutex<dyn AuditSink>> = Arc::new(Mutex::new(sink));

    let config = VMConfig {
        kernel: elf.to_path_buf(),
        memory_mb: 128,
        qmp: QmpEndpoint::tcp("127.0.0.1", qmp_port),
        serial: SerialEndpoint::tcp("127.0.0.1", serial_port),
        snapshot_dir: tmp.join("snapshots"),
        serial_observer: None,
        incoming_snapshot: None,
        incoming_relay_addr: None,
    };

    let mut vm = match RiscVVirtualMachine::new(config, audit) {
        Ok(vm) => vm,
        Err(e) => {
            failures
                .lock()
                .unwrap()
                .push(format!("round {round} guest {tag}: construct: {e}"));
            return;
        }
    };

    match vm.start() {
        Ok(()) => {}
        Err(e) => {
            failures.lock().unwrap().push(format!(
                "round {round} guest {tag}: start failed ({qmp_port}/{serial_port}): {e}"
            ));
            let _ = vm.stop();
            return;
        }
    }

    if !wait_for_serial(&vm, b"HELLO RISCV", Duration::from_secs(20)) {
        failures.lock().unwrap().push(format!(
            "round {round} guest {tag}: banner not captured ({:?})",
            String::from_utf8_lossy(&vm.serial_output())
        ));
    }
    let _ = vm.stop();
}

#[test]
#[ignore = "boots several QEMU guests repeatedly; run with --ignored"]
fn concurrent_boots_never_collide() {
    let elf = build_guest_elf();
    let failures = Mutex::new(Vec::new());

    for round in 1..=ROUNDS {
        println!("--- round {round} ({GUESTS} guests) ---");
        std::thread::scope(|scope| {
            for tag in 0..GUESTS {
                let failures = &failures;
                let elf = &elf;
                scope.spawn(move || boot_one(tag, elf, round, failures));
            }
        });
    }

    let failures = failures.into_inner().unwrap();
    for failure in &failures {
        println!("FAILURE: {failure}");
    }
    assert!(
        failures.is_empty(),
        "{} of {} concurrent boots failed",
        failures.len(),
        GUESTS * ROUNDS
    );
    println!(
        "port race stress: {0} concurrent boots, 0 failures",
        GUESTS * ROUNDS
    );
}
