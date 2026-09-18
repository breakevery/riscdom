//! `audit-rebuild` — rebuild the derived run index from the chain.
//!
//! ```text
//! audit-rebuild <path-to-db>
//! ```
//!
//! [`audit-verify`](super::verify) is deliberately read-only: a checker that can
//! write cannot be trusted as a checker, and a repair run would silently erase
//! the very divergence the cross-check exists to surface. The one operation that
//! writes to the derived index therefore lives in its own binary.
//!
//! It writes only to `runs` — never to `audit_events`, whose append-only triggers
//! stay in force. After rebuilding it runs the same cross-check as
//! `audit-verify --runs`, so a single command says whether the log's run
//! provenance is consistent.
//!
//! Exit codes:
//! - `0` — rebuilt, and the index agrees with the chain
//! - `1` — rebuilt, but the log still reports a problem: index findings, or run
//!   markers the chain cannot form into runs (a rebuild cannot invent the missing
//!   counterpart of an orphan marker)
//! - `2` — usage error or unreadable database

use audit::AuditStore;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: audit-rebuild <path-to-db>";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    if let Some(flag) = argv.iter().find(|arg| arg.starts_with('-')) {
        eprintln!("unknown option: {flag}");
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    if argv.len() != 1 {
        eprintln!("missing <path-to-db>");
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let path = &argv[0];

    let db = Path::new(path);
    if !db.exists() {
        eprintln!("failed to open {path}: no such file");
        return ExitCode::from(2);
    }
    let mut store = match AuditStore::open(db) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("failed to open {path}: {e}");
            return ExitCode::from(2);
        }
    };

    let report = match store.rebuild_run_index() {
        Ok(report) => report,
        Err(e) => {
            eprintln!("error: index rebuild failed: {e}");
            return ExitCode::from(2);
        }
    };
    println!(
        "IndexRebuilt {{ runs: {}, starts: {}, ends: {}, abandoned: {}, orphans: {} }}",
        report.runs, report.starts, report.ends, report.abandoned, report.orphans
    );

    let findings = match store.check_run_index() {
        Ok(findings) => findings,
        Err(e) => {
            eprintln!("error: run index check failed: {e}");
            return ExitCode::from(2);
        }
    };
    for finding in &findings {
        println!("  run index: {finding}");
    }
    println!("RunIndex {{ findings: {} }}", findings.len());
    if report.orphans > 0 {
        println!("ChainAnomalies {{ orphans: {} }}", report.orphans);
    }

    if findings.is_empty() && report.orphans == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
