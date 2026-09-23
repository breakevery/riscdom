//! `riscdom` — the command-line control-plane client.
//!
//! ```text
//! riscdom [--json] [--remote <host:port>] [--data-dir <dir>] [--workspace <dir>]
//!         [--token-file <path> | --token <value>] <command> [args]
//! ```
//!
//! Exit codes: `0` success, `1` a local failure, `2` usage or a rejected request,
//! `3` the control plane refused or failed, `4` authentication failed. `--help`
//! and `--version` are **not** errors: they print and exit `0`, as
//! `riscdom-server` already does.

use riscdom_cli::{args, run, USAGE};
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match args::parse(argv) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("riscdom: {message}");
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let mut out = std::io::stdout();
    let mut err = std::io::stderr();
    ExitCode::from(run(parsed, &mut out, &mut err))
}
