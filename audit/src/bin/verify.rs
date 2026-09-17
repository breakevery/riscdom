//! `audit-verify` — independently verify an audit log's hash chain.
//!
//! ```text
//! audit-verify <path-to-db> [--runs] [--rebuild-index]
//! ```
//!
//! - `--runs` — also cross-check the derived run index (`runs`) against the
//!   chain. Findings are printed; a divergence makes the exit code `1`.
//! - `--rebuild-index` — rebuild the derived index from the chain alone. Writes
//!   only to `runs`, never to `audit_events`.
//!
//! Exit codes:
//! - `0` — chain intact (and, with `--runs`, the index agrees with it)
//! - `1` — chain broken, or (with `--runs`) the index disagrees with the chain
//! - `2` — usage error, unreadable database, or an internal failure

use audit::{verify_chain, AuditStore, ChainStatus};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: audit-verify <path-to-db> [--runs] [--rebuild-index]";

struct Args {
    path: String,
    runs: bool,
    rebuild: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut path: Option<String> = None;
    let mut runs = false;
    let mut rebuild = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--runs" => runs = true,
            "--rebuild-index" => rebuild = true,
            other if other.starts_with('-') => return Err(format!("unknown option: {other}")),
            other => {
                if path.is_some() {
                    return Err(format!("unexpected extra argument: {other}"));
                }
                path = Some(other.to_string());
            }
        }
    }
    let path = path.ok_or_else(|| "missing <path-to-db>".to_string())?;
    Ok(Args {
        path,
        runs,
        rebuild,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    let db = Path::new(&args.path);
    if !db.exists() {
        eprintln!("failed to open {}: no such file", args.path);
        return ExitCode::from(2);
    }

    let mut store = match AuditStore::open(db) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("failed to open {}: {e}", args.path);
            return ExitCode::from(2);
        }
    };

    // Rebuild first, so a following `--runs` checks what was just written.
    if args.rebuild {
        match store.rebuild_run_index() {
            Ok(report) => println!(
                "IndexRebuilt {{ runs: {}, starts: {}, ends: {}, abandoned: {}, orphans: {} }}",
                report.runs, report.starts, report.ends, report.abandoned, report.orphans
            ),
            Err(e) => {
                eprintln!("error: index rebuild failed: {e}");
                return ExitCode::from(2);
            }
        }
    }

    let mut index_diverged = false;
    if args.runs {
        match store.check_run_index() {
            Ok(findings) => {
                for finding in &findings {
                    println!("  run index: {finding}");
                }
                println!("RunIndex {{ findings: {} }}", findings.len());
                index_diverged = !findings.is_empty();
            }
            Err(e) => {
                eprintln!("error: run index check failed: {e}");
                return ExitCode::from(2);
            }
        }
    }

    match verify_chain(&store) {
        Ok(ChainStatus::Intact { length }) => {
            println!("Intact {{ length: {length} }}");
            if index_diverged {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
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
