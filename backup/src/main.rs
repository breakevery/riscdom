//! `riscdom-backup` — the command line over [`riscdom_backup::export`] (v1.0 M7e, batch AV-1).
//!
//! ```
//! riscdom-backup export --data-dir <dir> --workspace <dir> --output <path> [--passphrase-from-env <VAR>] [--force]
//! ```
//!
//! The passphrase never comes from a command-line argument, is never written to disk by this tool,
//! and is never printed. Provide it through `--passphrase-from-env <VAR>` or on **stdin**; a
//! terminal prompt is a last resort and says so, because a terminal echoes what is typed.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
riscdom-backup — export a node's state as one encrypted package

usage:
  riscdom-backup export --data-dir <dir> --workspace <dir> --output <path> [--passphrase-from-env <VAR>] [--force]

options:
  --data-dir <dir>              the node's data directory (settings, sessions, token, node key, peers, rooms)
  --workspace <dir>             the node's workspace; its .riscdom/ holds the audit store and the snapshots
  --output <path>               where the package is written
  --passphrase-from-env <VAR>   read the passphrase from this environment variable
  --force                       overwrite an existing --output file
  -h, --help                    print this

The passphrase is read from --passphrase-from-env, else from stdin when it is piped. It is never a
command-line argument, never written to disk, and never printed.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("riscdom-backup: {message}");
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(message)) => {
            eprintln!("riscdom-backup: {message}");
            ExitCode::FAILURE
        }
    }
}

enum Failure {
    Usage(String),
    Error(String),
}

fn run(args: &[String]) -> Result<(), Failure> {
    let Some(command) = args.first() else {
        return Err(Failure::Usage("no command given".into()));
    };
    match command.as_str() {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        "export" => export(&args[1..]),
        other => Err(Failure::Usage(format!("unknown command `{other}`"))),
    }
}

fn export(rest: &[String]) -> Result<(), Failure> {
    let mut data_dir: Option<PathBuf> = None;
    let mut workspace: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut from_env: Option<String> = None;
    let mut force = false;

    let mut index = 0;
    while index < rest.len() {
        let flag = rest[index].as_str();
        index += 1;
        match flag {
            "--force" => force = true,
            "--passphrase-from-env" => {
                from_env = Some(value(rest, &mut index, flag)?);
            }
            "--data-dir" => data_dir = Some(PathBuf::from(value(rest, &mut index, flag)?)),
            "--workspace" => workspace = Some(PathBuf::from(value(rest, &mut index, flag)?)),
            "--output" => output = Some(PathBuf::from(value(rest, &mut index, flag)?)),
            other => return Err(Failure::Usage(format!("unknown option `{other}`"))),
        }
    }

    let data_dir = data_dir.ok_or_else(|| Failure::Usage("--data-dir is required".into()))?;
    let workspace = workspace.ok_or_else(|| Failure::Usage("--workspace is required".into()))?;
    let output = output.ok_or_else(|| Failure::Usage("--output is required".into()))?;
    if !data_dir.is_dir() {
        return Err(Failure::Error(format!(
            "--data-dir is not a directory: {}",
            data_dir.display()
        )));
    }
    if !workspace.is_dir() {
        return Err(Failure::Error(format!(
            "--workspace is not a directory: {}",
            workspace.display()
        )));
    }
    if output.exists() && !force {
        return Err(Failure::Error(format!(
            "--output already exists: {} (pass --force to replace it)",
            output.display()
        )));
    }

    let passphrase = read_passphrase(from_env.as_deref())?;
    let exported = riscdom_backup::export(&data_dir, &workspace, &passphrase)
        .map_err(|error| Failure::Error(error.to_string()))?;
    drop(passphrase);

    std::fs::write(&output, &exported.bytes)
        .map_err(|error| Failure::Error(format!("{}: {error}", output.display())))?;

    println!(
        "exported {} file(s) to {} ({} bytes)",
        exported.manifest.entries.len(),
        output.display(),
        exported.bytes.len()
    );
    for line in &exported.manifest.not_derived {
        println!("not carried: {line}");
    }
    Ok(())
}

/// The next value for a flag, or a usage failure.
fn value(rest: &[String], index: &mut usize, flag: &str) -> Result<String, Failure> {
    let value = rest
        .get(*index)
        .ok_or_else(|| Failure::Usage(format!("{flag} needs a value")))?;
    *index += 1;
    Ok(value.clone())
}

/// Read the passphrase without ever putting it on a command line.
///
/// Order: the named environment variable, else stdin when it is **piped**, else — only when stdin
/// is a terminal — an echoing prompt, with a warning, because hiding typed input portably would mean
/// another dependency and the two non-echoing sources are already here.
fn read_passphrase(from_env: Option<&str>) -> Result<Vec<u8>, Failure> {
    if let Some(name) = from_env {
        let value = std::env::var(name)
            .map_err(|_| Failure::Error(format!("the environment variable {name} is not set")))?;
        if value.is_empty() {
            return Err(Failure::Error(format!(
                "the environment variable {name} is empty"
            )));
        }
        return Ok(value.into_bytes());
    }

    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprintln!(
            "riscdom-backup: the terminal echoes what you type; prefer --passphrase-from-env <VAR> \
             or pipe the passphrase on stdin"
        );
        eprint!("passphrase: ");
        use std::io::Write;
        let _ = std::io::stderr().flush();
    }
    let mut buffer = String::new();
    stdin
        .lock()
        .read_to_string(&mut buffer)
        .map_err(|error| Failure::Error(format!("the passphrase could not be read: {error}")))?;
    let passphrase = buffer.trim_end_matches(['\r', '\n']).to_string();
    if passphrase.is_empty() {
        return Err(Failure::Error("the passphrase is empty".into()));
    }
    Ok(passphrase.into_bytes())
}
