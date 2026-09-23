//! The `riscdom` command line, parsed by hand.
//!
//! No argument-parsing crate is in the lock file and none is added for this:
//! `riscdom-server`, `worker`, `audit-verify` and `audit-rebuild` all parse their
//! own arguments, so the shape is established (a `USAGE` constant, a `parse`
//! function returning `Result`, and `ExitCode`).

use std::path::PathBuf;

/// What `--help` prints.
pub const USAGE: &str = "\
usage: riscdom [options] <command> [args]

commands:
  health                        the control plane's liveness (`GET /v0/health`)
  status                        connections, subscribers, agents (`GET /v0/status`)
  agents                        the agent count and identity (`GET /v0/status`)
  runs list [--limit <n>]       the run index, newest first (`GET /v0/runs`)
  runs get <run_id>             one run (`GET /v0/runs/<run_id>`)
  audit status                  event count and chain verdict (`GET /v0/audit/status`)
  audit events [--limit <n>]    recent audit events, newest first
  snapshots list                stored snapshots (`GET /v0/snapshots`)

options:
  --json                        print the control plane's JSON, unchanged
  --remote <host:port>          talk to a running riscdom-server instead of
                                starting one inside this process
  --data-dir <dir>              where settings, sessions and the token live
                                (default: this platform's host data dir)
  --workspace <dir>             the workspace the embedded server owns
                                (default: the current directory)
  --token-file <path>           read the bearer token from this file
  --token <value>               pass the bearer token on the command line
                                (warns: it lands in the shell history)
  --help, -h                    print this text
  --version, -V                 print the version

exit codes:
  0 success   1 local failure   2 usage or bad request
  3 the control plane refused or failed   4 authentication failed
";

/// One read-only command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Health,
    Status,
    Agents,
    RunsList { limit: Option<usize> },
    RunsGet { run_id: String },
    AuditStatus,
    AuditEvents { limit: usize },
    SnapshotsList,
}

impl Command {
    /// The request path this command asks for, query string included.
    pub fn request_path(&self) -> String {
        match self {
            Command::Health => "/v0/health".to_string(),
            Command::Status | Command::Agents => "/v0/status".to_string(),
            Command::RunsList { limit: None } => "/v0/runs".to_string(),
            Command::RunsList { limit: Some(limit) } => format!("/v0/runs?limit={limit}"),
            Command::RunsGet { run_id } => format!("/v0/runs/{}", url_encode(run_id)),
            Command::AuditStatus => "/v0/audit/status".to_string(),
            // `limit` is required by this endpoint, so the CLI always sends one.
            Command::AuditEvents { limit } => format!("/v0/audit/events?limit={limit}"),
            Command::SnapshotsList => "/v0/snapshots".to_string(),
        }
    }
}

/// The default `--limit` for `audit events`, which the endpoint requires.
pub const DEFAULT_EVENT_LIMIT: usize = 20;

/// The parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    /// `--remote host:port`, when given.
    pub remote: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub workspace: PathBuf,
    pub token_file: Option<PathBuf>,
    pub token: Option<String>,
    pub json: bool,
}

/// The result of parsing: a command, or one of the two informational flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Help,
    Version,
    Command(Args),
}

/// Parse the arguments after the program name.
///
/// Options may appear before, between or after the command words; the last
/// `--limit` wins. Anything unknown is an error rather than a guess.
pub fn parse(argv: Vec<String>) -> Result<Parsed, String> {
    let mut remote: Option<String> = None;
    let mut data_dir: Option<PathBuf> = None;
    let mut workspace: Option<PathBuf> = None;
    let mut token_file: Option<PathBuf> = None;
    let mut token: Option<String> = None;
    let mut json = false;
    let mut words: Vec<String> = Vec::new();
    let mut limit: Option<usize> = None;

    let mut args = argv.into_iter();
    while let Some(flag) = args.next() {
        let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--version" | "-V" => return Ok(Parsed::Version),
            "--json" => json = true,
            "--remote" => remote = Some(value("--remote")?),
            "--data-dir" => data_dir = Some(PathBuf::from(value("--data-dir")?)),
            "--workspace" => workspace = Some(PathBuf::from(value("--workspace")?)),
            "--token-file" => token_file = Some(PathBuf::from(value("--token-file")?)),
            "--token" => token = Some(value("--token")?),
            "--limit" => {
                let raw = value("--limit")?;
                limit = Some(
                    raw.parse()
                        .map_err(|e| format!("--limit {raw:?} is not a number: {e}"))?,
                );
            }
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(format!("unknown option {other:?}"));
            }
            other => words.push(other.to_string()),
        }
    }

    let command = parse_command(&words, limit)?;
    Ok(Parsed::Command(Args {
        command,
        remote,
        data_dir,
        workspace: workspace.unwrap_or_else(|| PathBuf::from(".")),
        token_file,
        token,
        json,
    }))
}

fn parse_command(words: &[String], limit: Option<usize>) -> Result<Command, String> {
    let word = |index: usize| words.get(index).map(String::as_str);
    match (word(0), word(1), word(2)) {
        (Some("health"), None, None) => Ok(Command::Health),
        (Some("status"), None, None) => Ok(Command::Status),
        (Some("agents"), None, None) => Ok(Command::Agents),
        (Some("runs"), Some("list"), None) => Ok(Command::RunsList { limit }),
        (Some("runs"), Some("get"), Some(run_id)) => Ok(Command::RunsGet {
            run_id: run_id.to_string(),
        }),
        (Some("runs"), Some("get"), None) => Err("runs get needs a <run_id>".to_string()),
        (Some("audit"), Some("status"), None) => Ok(Command::AuditStatus),
        (Some("audit"), Some("events"), None) => Ok(Command::AuditEvents {
            limit: limit.unwrap_or(DEFAULT_EVENT_LIMIT),
        }),
        (Some("snapshots"), Some("list"), None) => Ok(Command::SnapshotsList),
        (Some("health" | "status" | "agents" | "runs" | "audit" | "snapshots"), Some(other), _) => {
            Err(format!("unknown subcommand {other:?}"))
        }
        (None, _, _) => Err("missing <command>".to_string()),
        (Some(other), _, _) => Err(format!("unknown command {other:?}")),
    }
}

/// Percent-encode the characters that cannot appear raw in a path segment.
///
/// Run ids are `<device>-<pid>-<seq>`, so this is belt and braces; it keeps a
/// hand-copied id from turning into a different request.
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(args: &[&str]) -> Result<Parsed, String> {
        parse(args.iter().map(|a| a.to_string()).collect())
    }

    fn command(args: &[&str]) -> Command {
        match parse_words(args).expect("parses") {
            Parsed::Command(args) => args.command,
            other => panic!("not a command: {other:?}"),
        }
    }

    #[test]
    fn every_documented_command_parses() {
        assert_eq!(command(&["health"]), Command::Health);
        assert_eq!(command(&["status"]), Command::Status);
        assert_eq!(command(&["agents"]), Command::Agents);
        assert_eq!(
            command(&["runs", "list"]),
            Command::RunsList { limit: None }
        );
        assert_eq!(
            command(&["runs", "list", "--limit", "5"]),
            Command::RunsList { limit: Some(5) }
        );
        assert_eq!(
            command(&["runs", "get", "local-1-1"]),
            Command::RunsGet {
                run_id: "local-1-1".to_string()
            }
        );
        assert_eq!(command(&["audit", "status"]), Command::AuditStatus);
        assert_eq!(
            command(&["audit", "events"]),
            Command::AuditEvents {
                limit: DEFAULT_EVENT_LIMIT
            }
        );
        assert_eq!(
            command(&["audit", "events", "--limit", "3"]),
            Command::AuditEvents { limit: 3 }
        );
        assert_eq!(command(&["snapshots", "list"]), Command::SnapshotsList);
    }

    #[test]
    fn the_request_path_is_what_the_api_table_documents() {
        assert_eq!(Command::Health.request_path(), "/v0/health");
        assert_eq!(Command::Status.request_path(), "/v0/status");
        assert_eq!(Command::Agents.request_path(), "/v0/status");
        assert_eq!(Command::RunsList { limit: None }.request_path(), "/v0/runs");
        assert_eq!(
            Command::RunsList { limit: Some(7) }.request_path(),
            "/v0/runs?limit=7"
        );
        assert_eq!(
            Command::RunsGet {
                run_id: "local-1-1".into()
            }
            .request_path(),
            "/v0/runs/local-1-1"
        );
        assert_eq!(Command::AuditStatus.request_path(), "/v0/audit/status");
        assert_eq!(
            Command::AuditEvents { limit: 20 }.request_path(),
            "/v0/audit/events?limit=20"
        );
        assert_eq!(Command::SnapshotsList.request_path(), "/v0/snapshots");
        // A run id with a slash cannot smuggle a different path.
        assert_eq!(
            Command::RunsGet {
                run_id: "a/b".into()
            }
            .request_path(),
            "/v0/runs/a%2Fb"
        );
    }

    #[test]
    fn the_global_flags_parse_in_any_position() {
        let parsed = parse_words(&[
            "--json",
            "--remote",
            "127.0.0.1:7821",
            "runs",
            "list",
            "--limit",
            "9",
            "--data-dir",
            "/tmp/data",
            "--workspace",
            "/tmp/ws",
            "--token-file",
            "/tmp/token",
        ])
        .expect("parses");
        let args = match parsed {
            Parsed::Command(args) => args,
            other => panic!("not a command: {other:?}"),
        };
        assert!(args.json);
        assert_eq!(args.remote.as_deref(), Some("127.0.0.1:7821"));
        assert_eq!(args.data_dir, Some(PathBuf::from("/tmp/data")));
        assert_eq!(args.workspace, PathBuf::from("/tmp/ws"));
        assert_eq!(args.token_file, Some(PathBuf::from("/tmp/token")));
        assert_eq!(args.command, Command::RunsList { limit: Some(9) });
    }

    #[test]
    fn the_workspace_defaults_to_the_current_directory() {
        let parsed = parse_words(&["health"]).expect("parses");
        let args = match parsed {
            Parsed::Command(args) => args,
            other => panic!("not a command: {other:?}"),
        };
        assert_eq!(args.workspace, PathBuf::from("."));
        assert!(args.remote.is_none());
        assert!(!args.json);
    }

    #[test]
    fn help_and_version_are_not_commands() {
        assert_eq!(parse_words(&["--help"]).expect("parses"), Parsed::Help);
        assert_eq!(parse_words(&["-h"]).expect("parses"), Parsed::Help);
        assert_eq!(
            parse_words(&["--version"]).expect("parses"),
            Parsed::Version
        );
        assert_eq!(parse_words(&["-V"]).expect("parses"), Parsed::Version);
        // Even after a command: asking for help is never running it.
        assert_eq!(
            parse_words(&["runs", "list", "--help"]).expect("parses"),
            Parsed::Help
        );
    }

    #[test]
    fn an_unknown_or_incomplete_command_line_is_refused() {
        for args in [
            vec![],
            vec!["nope"],
            vec!["runs"],
            vec!["runs", "nope"],
            vec!["runs", "get"],
            vec!["audit"],
            vec!["snapshots"],
            vec!["health", "extra"],
            vec!["--nope"],
            vec!["--remote"],
            vec!["--limit"],
            vec!["audit", "events", "--limit", "soon"],
        ] {
            assert!(parse_words(&args).is_err(), "{args:?} should not parse");
        }
    }
}
