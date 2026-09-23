//! The `riscdom` command line, parsed by hand.
//!
//! No argument-parsing crate is in the lock file and none is added for this:
//! `riscdom-server`, `worker`, `audit-verify` and `audit-rebuild` all parse their
//! own arguments, so the shape is established (a `USAGE` constant, a `parse`
//! function returning `Result`, and `ExitCode`).

use serde_json::json;
use std::path::PathBuf;

/// What `--help` prints.
pub const USAGE: &str = "\
usage: riscdom [options] <command> [args]

read-only commands:
  health                        the control plane's liveness (`GET /v0/health`)
  status                        connections, subscribers, agents (`GET /v0/status`)
  agents                        the agent count and identity (`GET /v0/status`)
  runs list [--limit <n>]       the run index, newest first (`GET /v0/runs`)
  runs get <run_id>             one run (`GET /v0/runs/<run_id>`)
  audit status                  event count and chain verdict (`GET /v0/audit/status`)
  audit events [--limit <n>]    recent audit events, newest first
  snapshots list                stored snapshots (`GET /v0/snapshots`)

control commands:
  run <task> [--follow]         run one agent turn; --follow prints the event
                                stream while it runs
  vm stop                       stop the host's VM (asks for confirmation)
  vm start                      reserved; the control plane answers 501
  snapshots save <name>         save a snapshot of the running VM
  snapshots resume <name>       restore one (asks for confirmation: it stops the
                                VM first)
  snapshots delete <name>       delete one (asks for confirmation)
  sessions create <title>       start a session
  sessions open <session_id>    one session and its messages
  sessions rename <id> <title>  retitle a session
  sessions delete <session_id>  delete one (asks for confirmation)
  sessions clear-all            delete every session (asks for confirmation)
  runs abandon-stale            mark abandoned runs (idempotent)

options:
  --json                        print the control plane's JSON, unchanged
  --yes                         confirm a destructive command without a prompt
                                (required when stdin is not a terminal)
  --follow                      `run` only: print the event stream while running
  --remote <host:port>          talk to a running riscdom-server instead of
                                starting one inside this process
  --data-dir <dir>              where settings, sessions and the token live
                                (default: this platform's host data dir)
  --workspace <dir>             the workspace the embedded server owns
                                (default: the current directory)
  --token-file <path>           read the bearer token from this file
  --token <value>               pass the bearer token on the command line
                                (warns: it lands in the shell history)
  --limit <n>                   how many rows `runs list` / `audit events` want
  --help, -h                    print this text
  --version, -V                 print the version

exit codes:
  0 success   1 local failure   2 usage, refused confirmation, or bad request
  3 the control plane refused or failed   4 authentication failed
";

/// One command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // ---- read-only ----
    Health,
    Status,
    Agents,
    RunsList { limit: Option<usize> },
    RunsGet { run_id: String },
    AuditStatus,
    AuditEvents { limit: usize },
    SnapshotsList,
    // ---- control ----
    Run { task: String },
    VmStop,
    VmStart,
    SnapshotsSave { name: String },
    SnapshotsResume { name: String },
    SnapshotsDelete { name: String },
    SessionsCreate { title: String },
    SessionsOpen { session_id: String },
    SessionsDelete { session_id: String },
    SessionsRename { session_id: String, title: String },
    SessionsClearAll,
    RunsAbandonStale,
}

impl Command {
    /// The HTTP method the control plane expects for this command.
    pub fn method(&self) -> &'static str {
        match self {
            Command::Health
            | Command::Status
            | Command::Agents
            | Command::RunsList { .. }
            | Command::RunsGet { .. }
            | Command::AuditStatus
            | Command::AuditEvents { .. }
            | Command::SnapshotsList => "GET",
            _ => "POST",
        }
    }

    /// The request path, query string included.
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
            Command::Run { .. } => "/v0/agent/run".to_string(),
            Command::VmStop => "/v0/vm/stop".to_string(),
            Command::VmStart => "/v0/vm/start".to_string(),
            Command::SnapshotsSave { .. } => "/v0/snapshots/save".to_string(),
            Command::SnapshotsResume { .. } => "/v0/snapshots/resume".to_string(),
            Command::SnapshotsDelete { .. } => "/v0/snapshots/delete".to_string(),
            Command::SessionsCreate { .. } => "/v0/sessions/create".to_string(),
            Command::SessionsOpen { .. } => "/v0/sessions/open".to_string(),
            Command::SessionsDelete { .. } => "/v0/sessions/delete".to_string(),
            Command::SessionsRename { .. } => "/v0/sessions/rename".to_string(),
            Command::SessionsClearAll => "/v0/sessions/clear".to_string(),
            Command::RunsAbandonStale => "/v0/runs/abandon-stale".to_string(),
        }
    }

    /// The JSON body, or `None` for the commands that take none.
    pub fn body(&self) -> Option<serde_json::Value> {
        match self {
            Command::Run { task } => Some(json!({ "user_input": task })),
            Command::SnapshotsSave { name }
            | Command::SnapshotsResume { name }
            | Command::SnapshotsDelete { name } => Some(json!({ "name": name })),
            Command::SessionsCreate { title } => Some(json!({ "title": title })),
            Command::SessionsOpen { session_id } | Command::SessionsDelete { session_id } => {
                Some(json!({ "session_id": session_id }))
            }
            Command::SessionsRename { session_id, title } => {
                Some(json!({ "session_id": session_id, "title": title }))
            }
            _ => None,
        }
    }

    /// The question to ask before running this command, when it destroys
    /// something. `None` means it does not.
    pub fn confirmation(&self) -> Option<String> {
        match self {
            Command::VmStop => Some("Stop the VM? A run in flight is interrupted.".to_string()),
            // Restoring stops the current VM first, so it interrupts like `vm stop`.
            Command::SnapshotsResume { name } => Some(format!(
                "Resume from snapshot {name:?}? The current VM is stopped first."
            )),
            Command::SnapshotsDelete { name } => Some(format!("Delete snapshot {name:?}?")),
            Command::SessionsDelete { session_id } => {
                Some(format!("Delete session {session_id:?}?"))
            }
            Command::SessionsClearAll => Some("Delete every session?".to_string()),
            _ => None,
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
    /// `--token`, when given.
    pub token: Option<String>,
    pub json: bool,
    /// `--yes`: the confirmation is given up front.
    pub yes: bool,
    /// `--follow`: `run` prints the event stream while it runs.
    pub follow: bool,
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
    let mut yes = false;
    let mut follow = false;
    let mut words: Vec<String> = Vec::new();
    let mut limit: Option<usize> = None;

    let mut args = argv.into_iter();
    while let Some(flag) = args.next() {
        let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--version" | "-V" => return Ok(Parsed::Version),
            "--json" => json = true,
            "--yes" | "-y" => yes = true,
            "--follow" | "-f" => follow = true,
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
    if follow && !matches!(command, Command::Run { .. }) {
        return Err("--follow is only meaningful for `run`".to_string());
    }
    Ok(Parsed::Command(Args {
        command,
        remote,
        data_dir,
        workspace: workspace.unwrap_or_else(|| PathBuf::from(".")),
        token_file,
        token,
        json,
        yes,
        follow,
    }))
}

fn parse_command(words: &[String], limit: Option<usize>) -> Result<Command, String> {
    let (w0, w1, w2, w3) = (
        words.first().map(String::as_str),
        words.get(1).map(String::as_str),
        words.get(2).map(String::as_str),
        words.get(3).map(String::as_str),
    );
    // Exactly the accepted shapes come first; everything else is refused below,
    // with the most specific message we can give.
    let accepted = match (w0, w1, w2, w3) {
        (Some("health"), None, None, None) => Some(Command::Health),
        (Some("status"), None, None, None) => Some(Command::Status),
        (Some("agents"), None, None, None) => Some(Command::Agents),
        (Some("runs"), Some("list"), None, None) => Some(Command::RunsList { limit }),
        (Some("runs"), Some("get"), Some(run_id), None) => Some(Command::RunsGet {
            run_id: run_id.to_string(),
        }),
        (Some("audit"), Some("status"), None, None) => Some(Command::AuditStatus),
        (Some("audit"), Some("events"), None, None) => Some(Command::AuditEvents {
            limit: limit.unwrap_or(DEFAULT_EVENT_LIMIT),
        }),
        (Some("snapshots"), Some("list"), None, None) => Some(Command::SnapshotsList),
        (Some("run"), Some(task), None, None) => Some(Command::Run {
            task: task.to_string(),
        }),
        (Some("vm"), Some("stop"), None, None) => Some(Command::VmStop),
        (Some("vm"), Some("start"), None, None) => Some(Command::VmStart),
        (Some("snapshots"), Some("save"), Some(name), None) => Some(Command::SnapshotsSave {
            name: name.to_string(),
        }),
        (Some("snapshots"), Some("resume"), Some(name), None) => Some(Command::SnapshotsResume {
            name: name.to_string(),
        }),
        (Some("snapshots"), Some("delete"), Some(name), None) => Some(Command::SnapshotsDelete {
            name: name.to_string(),
        }),
        (Some("sessions"), Some("create"), Some(title), None) => Some(Command::SessionsCreate {
            title: title.to_string(),
        }),
        (Some("sessions"), Some("open"), Some(session_id), None) => Some(Command::SessionsOpen {
            session_id: session_id.to_string(),
        }),
        (Some("sessions"), Some("delete"), Some(session_id), None) => {
            Some(Command::SessionsDelete {
                session_id: session_id.to_string(),
            })
        }
        (Some("sessions"), Some("rename"), Some(session_id), Some(title)) => {
            Some(Command::SessionsRename {
                session_id: session_id.to_string(),
                title: title.to_string(),
            })
        }
        (Some("sessions"), Some("clear-all"), None, None) => Some(Command::SessionsClearAll),
        (Some("runs"), Some("abandon-stale"), None, None) => Some(Command::RunsAbandonStale),
        _ => None,
    };
    if let Some(command) = accepted {
        return Ok(command);
    }

    match (w0, w1, w2) {
        (None, _, _) => Err("missing <command>".to_string()),
        (Some("run"), None, _) => Err("run needs a <task>".to_string()),
        (Some("runs"), Some("get"), None) => Err("runs get needs a <run_id>".to_string()),
        (Some("sessions"), Some("rename"), Some(_)) if w3.is_none() => {
            Err("sessions rename needs a <title>".to_string())
        }
        (Some("run" | "snapshots" | "sessions"), Some(subcommand), None)
            if subcommand != "list" =>
        {
            Err(format!("{subcommand:?} needs an argument"))
        }
        (Some(command), Some(other), _) => Err(format!("unknown or incomplete: {command} {other}")),
        (Some(command), None, _) => Err(format!("unknown command {command:?}")),
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

    fn args_of(args: &[&str]) -> Args {
        match parse_words(args).expect("parses") {
            Parsed::Command(args) => args,
            other => panic!("not a command: {other:?}"),
        }
    }

    fn command(args: &[&str]) -> Command {
        args_of(args).command
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
        // control commands
        assert_eq!(
            command(&["run", "say hi"]),
            Command::Run {
                task: "say hi".to_string()
            }
        );
        assert_eq!(command(&["vm", "stop"]), Command::VmStop);
        assert_eq!(command(&["vm", "start"]), Command::VmStart);
        assert_eq!(
            command(&["snapshots", "save", "after-blink"]),
            Command::SnapshotsSave {
                name: "after-blink".to_string()
            }
        );
        assert_eq!(
            command(&["snapshots", "resume", "after-blink"]),
            Command::SnapshotsResume {
                name: "after-blink".to_string()
            }
        );
        assert_eq!(
            command(&["snapshots", "delete", "after-blink"]),
            Command::SnapshotsDelete {
                name: "after-blink".to_string()
            }
        );
        assert_eq!(
            command(&["sessions", "create", "work"]),
            Command::SessionsCreate {
                title: "work".to_string()
            }
        );
        assert_eq!(
            command(&["sessions", "open", "s-1"]),
            Command::SessionsOpen {
                session_id: "s-1".to_string()
            }
        );
        assert_eq!(
            command(&["sessions", "delete", "s-1"]),
            Command::SessionsDelete {
                session_id: "s-1".to_string()
            }
        );
        assert_eq!(
            command(&["sessions", "rename", "s-1", "later"]),
            Command::SessionsRename {
                session_id: "s-1".to_string(),
                title: "later".to_string()
            }
        );
        assert_eq!(
            command(&["sessions", "clear-all"]),
            Command::SessionsClearAll
        );
        assert_eq!(
            command(&["runs", "abandon-stale"]),
            Command::RunsAbandonStale
        );
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
        assert_eq!(
            Command::Run { task: "x".into() }.request_path(),
            "/v0/agent/run"
        );
        assert_eq!(Command::VmStop.request_path(), "/v0/vm/stop");
        assert_eq!(Command::VmStart.request_path(), "/v0/vm/start");
        assert_eq!(
            Command::SnapshotsSave { name: "a".into() }.request_path(),
            "/v0/snapshots/save"
        );
        assert_eq!(
            Command::SnapshotsResume { name: "a".into() }.request_path(),
            "/v0/snapshots/resume"
        );
        assert_eq!(
            Command::SnapshotsDelete { name: "a".into() }.request_path(),
            "/v0/snapshots/delete"
        );
        assert_eq!(
            Command::SessionsCreate { title: "t".into() }.request_path(),
            "/v0/sessions/create"
        );
        assert_eq!(
            Command::SessionsOpen {
                session_id: "s".into()
            }
            .request_path(),
            "/v0/sessions/open"
        );
        assert_eq!(
            Command::SessionsDelete {
                session_id: "s".into()
            }
            .request_path(),
            "/v0/sessions/delete"
        );
        assert_eq!(
            Command::SessionsRename {
                session_id: "s".into(),
                title: "t".into()
            }
            .request_path(),
            "/v0/sessions/rename"
        );
        assert_eq!(
            Command::SessionsClearAll.request_path(),
            "/v0/sessions/clear"
        );
        assert_eq!(
            Command::RunsAbandonStale.request_path(),
            "/v0/runs/abandon-stale"
        );
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
    fn the_methods_and_bodies_match_the_table() {
        assert_eq!(Command::Health.method(), "GET");
        assert_eq!(Command::SnapshotsList.method(), "GET");
        assert_eq!(Command::Run { task: "t".into() }.method(), "POST");
        assert_eq!(Command::VmStop.method(), "POST");
        assert_eq!(
            Command::Run { task: "t".into() }.body(),
            Some(json!({ "user_input": "t" }))
        );
        assert_eq!(
            Command::SnapshotsSave { name: "a".into() }.body(),
            Some(json!({ "name": "a" }))
        );
        assert_eq!(
            Command::SessionsRename {
                session_id: "s".into(),
                title: "t".into()
            }
            .body(),
            Some(json!({ "session_id": "s", "title": "t" }))
        );
        // The commands with no parameters send no body at all.
        assert_eq!(Command::VmStop.body(), None);
        assert_eq!(Command::SessionsClearAll.body(), None);
        assert_eq!(Command::RunsAbandonStale.body(), None);
        assert_eq!(Command::Health.body(), None);
    }

    #[test]
    fn only_the_destructive_commands_ask_first() {
        assert!(Command::VmStop.confirmation().is_some());
        assert!(Command::SnapshotsResume { name: "a".into() }
            .confirmation()
            .is_some());
        assert!(Command::SnapshotsDelete { name: "a".into() }
            .confirmation()
            .is_some());
        assert!(Command::SessionsDelete {
            session_id: "s".into()
        }
        .confirmation()
        .is_some());
        assert!(Command::SessionsClearAll.confirmation().is_some());
        // Idempotent or additive: no question.
        assert_eq!(Command::RunsAbandonStale.confirmation(), None);
        assert_eq!(
            Command::SnapshotsSave { name: "a".into() }.confirmation(),
            None
        );
        assert_eq!(
            Command::SessionsCreate { title: "t".into() }.confirmation(),
            None
        );
        assert_eq!(Command::Run { task: "t".into() }.confirmation(), None);
    }

    #[test]
    fn the_global_flags_parse_in_any_position() {
        let args = args_of(&[
            "--json",
            "--yes",
            "--remote",
            "127.0.0.1:7821",
            "sessions",
            "clear-all",
            "--data-dir",
            "/tmp/data",
            "--workspace",
            "/tmp/ws",
            "--token-file",
            "/tmp/token",
        ]);
        assert!(args.json);
        assert!(args.yes);
        assert_eq!(args.remote.as_deref(), Some("127.0.0.1:7821"));
        assert_eq!(args.data_dir, Some(PathBuf::from("/tmp/data")));
        assert_eq!(args.workspace, PathBuf::from("/tmp/ws"));
        assert_eq!(args.token_file, Some(PathBuf::from("/tmp/token")));
        assert_eq!(args.command, Command::SessionsClearAll);
    }

    #[test]
    fn follow_belongs_to_run_and_nothing_else() {
        let args = args_of(&["run", "hi", "--follow"]);
        assert!(args.follow);
        assert!(args_of(&["--follow", "run", "hi"]).follow);
        assert!(!args_of(&["run", "hi"]).follow);
        for args in [vec!["health", "--follow"], vec!["--follow", "status"]] {
            let refused = parse_words(&args);
            assert!(refused.is_err(), "{args:?} should not parse");
            assert!(
                refused.expect_err("refused").contains("--follow"),
                "{args:?}"
            );
        }
    }

    #[test]
    fn the_workspace_defaults_to_the_current_directory() {
        let args = args_of(&["health"]);
        assert_eq!(args.workspace, PathBuf::from("."));
        assert!(args.remote.is_none());
        assert!(!args.json);
        assert!(!args.yes);
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
            vec!["snapshots", "get"],
            vec!["health", "extra"],
            vec!["run"],
            vec!["vm"],
            vec!["vm", "restart"],
            vec!["sessions"],
            vec!["sessions", "rename", "s-1"],
            vec!["sessions", "clear-all", "now"],
            vec!["--nope"],
            vec!["--remote"],
            vec!["--limit"],
            vec!["audit", "events", "--limit", "soon"],
        ] {
            assert!(parse_words(&args).is_err(), "{args:?} should not parse");
        }
    }
}
