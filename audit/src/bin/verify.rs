//! `audit-verify` — independently verify an audit log's hash chain.
//!
//! ```text
//! audit-verify <path-to-db> [--runs]
//! ```
//!
//! - `--runs` — also cross-check the derived run index (`runs`) against the
//!   chain. Findings are printed; a divergence makes the exit code `1`.
//!
//! This binary is **read-only by design**: a checker that can write cannot be
//! trusted as a checker, and a repair run would silently erase the very
//! divergence `--runs` exists to surface. Rebuilding the derived index lives in
//! the separate `audit-rebuild` binary.
//!
//! Exit codes:
//! - `0` — chain intact (and, with `--runs`, the index agrees with it)
//! - `1` — chain broken, or (with `--runs`) the index disagrees with the chain
//! - `2` — usage error, unreadable database, or an internal failure

use audit::{verify_chain, AuditStore, ChainStatus};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: audit-verify <path-to-db> [--runs]";

struct Args {
    path: String,
    runs: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut path: Option<String> = None;
    let mut runs = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--runs" => runs = true,
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
    Ok(Args { path, runs })
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

    let store = match AuditStore::open(db) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("failed to open {}: {e}", args.path);
            return ExitCode::from(2);
        }
    };

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
