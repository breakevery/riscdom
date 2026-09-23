//! riscdom — the command-line control-plane client.
//!
//! The CLI is a **client of the control plane**, not a second way into the
//! kernel: every command goes through HTTP, and the local mode simply starts the
//! control plane inside this process, on a loopback port the OS picks. That is
//! what keeps `riscdom` and `riscdom --remote host:port` the same code path (the
//! decision is in `docs/decisions.md` §8).
//!
//! ```text
//! riscdom [--json] [--remote <host:port>] [--data-dir <dir>] [--workspace <dir>]
//!         [--token-file <path> | --token <value>] <command> [args]
//! ```
//!
//! Exit codes (the map is `client::exit_code_for` and the table is in
//! `cli/README.md`): `0` success, `1` a local failure (no connection, no token,
//! no workspace), `2` a usage or request error, `3` the control plane refused or
//! failed, `4` authentication failed.

pub mod args;
pub mod client;
pub mod render;

use std::io::Write;

pub use args::{Args, Command, Parsed, USAGE};
pub use client::{exit_code_for, Client, Embedded, Error, Reply, Session};

/// Run one parsed command line, writing to `out` on success and `err` on
/// failure. Returns the process exit code, because that is the CLI's contract.
///
/// Taking the streams as arguments is what lets the unit tests drive every
/// branch without spawning a process; `main` passes stdout and stderr.
pub fn run(parsed: Parsed, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match parsed {
        Parsed::Help => {
            let _ = write!(out, "{USAGE}");
            return 0;
        }
        Parsed::Version => {
            let _ = writeln!(out, "riscdom {}", env!("CARGO_PKG_VERSION"));
            return 0;
        }
        Parsed::Command(args) => args,
    };

    // One session: remote, or the control plane embedded in this process.
    let session = match Session::open(&args) {
        Ok(session) => session,
        Err(error) => return report(&args, &error, err),
    };

    let path = args.command.request_path();
    let reply = match session.get(&path) {
        Ok(reply) => reply,
        Err(error) => return report(&args, &error, err),
    };

    if reply.is_success() {
        if args.json {
            // Pass the control plane's JSON through untouched.
            let _ = writeln!(out, "{}", reply.body);
        } else {
            let _ = writeln!(out, "{}", render::human(&args.command, &reply));
        }
        0
    } else {
        report(&args, &Error::from_reply(reply), err)
    }
}

/// Print a failure the way the mode asks for, and return its exit code.
fn report(args: &Args, error: &Error, err: &mut dyn Write) -> u8 {
    if args.json {
        // A server error body is already the documented shape; a local failure is
        // wrapped in it so a JSON consumer never has to parse prose.
        let _ = writeln!(err, "{}", error.json());
    } else {
        let _ = writeln!(err, "{}", error.human());
    }
    error.code
}
