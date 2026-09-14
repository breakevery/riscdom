//! Print a small real hash chain (for inspection / documentation).
//!
//! ```text
//! cargo run -p audit --example chain_demo
//! ```

use audit::{AuditEvent, AuditStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join("riscdom-audit-demo");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("demo.db");
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.join(format!("demo.db{suffix}")));
    }

    let mut store = AuditStore::open(&path)?;
    for (actor, action) in [
        ("sandbox", "vm.start"),
        ("sandbox", "serial.read"),
        ("sandbox", "vm.stop"),
    ] {
        store.append(AuditEvent::new(
            actor,
            action,
            serde_json::json!({ "demo": true }),
        ))?;
    }

    let columns = ["id", "actor", "action", "prev_hash", "hash"];
    println!(
        "{:<3} {:<9} {:<13} {:<64} {}",
        columns[0], columns[1], columns[2], columns[3], columns[4]
    );
    for e in store.all()? {
        println!(
            "{:<3} {:<9} {:<13} {:<64} {}",
            e.id, e.event.actor, e.event.action, e.prev_hash, e.hash
        );
    }
    println!("\nverify: {:?}", audit::verify_chain(&store)?);
    Ok(())
}
