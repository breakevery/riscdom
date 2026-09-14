//! `audit-verify` — independently verify an audit log's hash chain.
//!
//! ```text
//! audit-verify <path-to-db>
//! ```
//!
//! Exit codes:
//! - `0` — chain intact
//! - `1` — chain broken
//! - `2` — could not open the database

use audit::{verify_chain, AuditStore, ChainStatus};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let path = match std::env::args().nth(1) {
        Some(p) => p,
        None => {
            eprintln!("usage: audit-verify <path-to-db>");
            return ExitCode::from(2);
        }
    };

    let db = Path::new(&path);
    if !db.exists() {
        eprintln!("failed to open {path}: no such file");
        return ExitCode::from(2);
    }

    let store = match AuditStore::open(db) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to open {path}: {e}");
            return ExitCode::from(2);
        }
    };

    match verify_chain(&store) {
        Ok(ChainStatus::Intact { length }) => {
            println!("Intact {{ length: {length} }}");
            ExitCode::SUCCESS
        }
        Ok(ChainStatus::Broken { at_id, reason }) => {
            println!("Broken {{ at_id: {at_id}, reason: \"{reason}\" }}");
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}
