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
  sandboxes list                the sandbox registry: hand-written, scanned, and
                                the built-in fallback (`GET /v0/sandboxes`)
  sandboxes current             the definition a run would use, and the fallback
  sandboxes candidates          what is installed on this machine (the raw scan)
  sandboxes show <name>         one definition (`GET /v0/sandboxes/<name>`)
  sandboxes switch <name>       switch this node to that definition (asks first:
                                the running VM is stopped and started again)
                                (`POST /v0/sandboxes/switch`)
  sandboxes requests [--status <s>]
                                the sandbox requests waiting for a decision
                                (`GET /v0/sandboxes/requests`)
  workspace export [--out <file>]
                                the project as a tar.gz; without --out it goes to
                                stdout, and the count goes to stderr

control commands:
  run <task> [--follow] [--sandbox <name>]
                                run one agent turn; --follow prints the event
                                stream while it runs, and --sandbox declares which
                                definition this run uses (the node is not switched)
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
  sandboxes requests approve <id>
  sandboxes requests reject <id>
                                decide a sandbox request (asks first: a decision is
                                a permission action, and it is not reversible)
  workspace import <archive> [--force]
                                unpack a project archive into the workspace; an
                                existing file is kept unless --force says otherwise

export commands:
  export audit-jsonl [--out <path>]         the whole audit chain as JSONL
  export run-audit <run_id> [--out <path>]  one run's self-contained chain
  export serial-log [--out <path>]          the captured serial output
                                The path belongs to the *server*: it is resolved
                                against the workspace root, and a path outside
                                it is refused. The defaults are audit.jsonl,
                                run-<run_id>.jsonl and serial.log

configuration commands:
  llm set --api-key <key> --base-url <url> --model <model>
                                [--provider-id <id>] [--remember]
                                configure the model; --api-key-file <path> reads
                                the key from a file instead of the command line
  llm clear                     forget the model configuration (asks)
  llm load-key <provider_id>    load a stored key from the OS credential store
  qemu path <file>              use this QEMU binary
  qemu clear                    forget it (asks)
  qemu download [--wait]        install the pinned QEMU build; --wait prints
                                progress until it finishes (today this refuses and
                                prints how to install QEMU yourself)
  qemu cancel                   cancel a running QEMU download
  qemu status                   whether a QEMU download is running
  toolchain download [--wait]   download the pinned RISC-V toolchain; --wait
                                prints progress until it finishes
  toolchain cancel              cancel a running download
  toolchain path <file>         use this compiler
  toolchain clear               forget it (asks)
  preflight run [--wait]        check the environment; --wait prints every step
  preflight ack                 accept the current configuration as it is
  audit alert set <on|off>      the audit-failure alert
  theme set <light|dark|system> the interface theme
  language set <system|en|zh>   the interface language

options:
  --json                        print the control plane's JSON, unchanged
  --yes                         confirm a destructive command without a prompt
                                (required when stdin is not a terminal)
  --follow                      `run` only: print the event stream while running
  --wait                        `toolchain download` / `preflight run` / `qemu
                                download` only: print progress until the work
                                finishes
  --out <path>                  where an export writes (server-side, resolved
                                against the workspace root)
  --api-key <key>               the model's API key (warns: it lands in the
                                shell history)
  --api-key-file <path>         read the API key from this file instead
  --base-url <url>              the model endpoint
  --model <model>               the model's name
  --provider-id <id>            the provider preset `llm set` configures
  --remember                    `llm set`: also store the key in the OS
                                credential store
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
    RunsList {
        limit: Option<usize>,
    },
    RunsGet {
        run_id: String,
    },
    AuditStatus,
    AuditEvents {
        limit: usize,
    },
    SnapshotsList,
    /// The merged sandbox registry (v0.9 sandbox F2a-2), read-only.
    SandboxesList,
    SandboxesCurrent,
    SandboxesCandidates,
    SandboxesShow {
        name: String,
    },
    /// The one sandbox write: switch this node to another definition (F2b-2).
    SandboxesSwitch {
        name: String,
    },
    /// The request queue (v0.9 sandbox F2c), read-only, optionally filtered.
    SandboxesRequests {
        status: Option<String>,
    },
    /// The two decisions. Both need the capability the request's action implies,
    /// so both lean on the server's answer rather than guessing here.
    SandboxesRequestsApprove {
        id: String,
    },
    SandboxesRequestsReject {
        id: String,
    },
    /// Project in/out (v0.9): an archive goes to the host, an archive comes back.
    WorkspaceImport {
        /// The local file that holds the archive.
        archive: PathBuf,
        /// `--force`: replace files that are already in the workspace.
        force: bool,
    },
    WorkspaceExport {
        /// Where the archive is written; `None` means stdout.
        out: Option<PathBuf>,
    },
    // ---- control ----
    Run {
        task: String,
        /// `--sandbox <name>`: the definition this run declares (v0.9 sandbox F2d).
        sandbox: Option<String>,
    },
    VmStop,
    VmStart,
    SnapshotsSave {
        name: String,
    },
    SnapshotsResume {
        name: String,
    },
    SnapshotsDelete {
        name: String,
    },
    SessionsCreate {
        title: String,
    },
    SessionsOpen {
        session_id: String,
    },
    SessionsDelete {
        session_id: String,
    },
    SessionsRename {
        session_id: String,
        title: String,
    },
    SessionsClearAll,
    RunsAbandonStale,
    // ---- export ----
    ExportAuditJsonl {
        out: String,
    },
    ExportRunAudit {
        run_id: String,
        out: String,
    },
    ExportSerialLog {
        out: String,
    },
    // ---- configuration ----
    LlmSet {
        /// `--api-key`, when given. `None` means `--api-key-file` was used and
        /// `lib::run` has not read the file yet.
        api_key: Option<String>,
        api_key_file: Option<PathBuf>,
        base_url: String,
        model: String,
        provider_id: Option<String>,
        remember: bool,
    },
    LlmClear,
    LlmLoadKey {
        provider_id: String,
    },
    QemuPath {
        path: String,
    },
    QemuClear,
    QemuDownload,
    QemuCancel,
    QemuStatus,
    ToolchainDownload,
    ToolchainCancel,
    ToolchainPath {
        path: String,
    },
    ToolchainClear,
    PreflightRun,
    PreflightAck,
    AuditAlertSet {
        enabled: bool,
    },
    ThemeSet {
        theme: String,
    },
    LanguageSet {
        language: String,
    },
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
            | Command::SnapshotsList
            | Command::SandboxesList
            | Command::SandboxesCurrent
            | Command::SandboxesCandidates
            | Command::SandboxesShow { .. }
            | Command::SandboxesRequests { .. }
            | Command::QemuStatus => "GET",
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
            Command::SandboxesList => "/v0/sandboxes".to_string(),
            Command::SandboxesCurrent => "/v0/sandboxes/current".to_string(),
            Command::SandboxesCandidates => "/v0/sandboxes/candidates".to_string(),
            Command::SandboxesShow { name } => {
                format!("/v0/sandboxes/{}", url_encode(name))
            }
            Command::SandboxesSwitch { .. } => "/v0/sandboxes/switch".to_string(),
            Command::SandboxesRequests { status: None } => "/v0/sandboxes/requests".to_string(),
            Command::SandboxesRequests {
                status: Some(status),
            } => {
                format!("/v0/sandboxes/requests?status={}", url_encode(status))
            }
            Command::SandboxesRequestsApprove { id } => {
                format!("/v0/sandboxes/requests/{}/approve", url_encode(id))
            }
            Command::SandboxesRequestsReject { id } => {
                format!("/v0/sandboxes/requests/{}/reject", url_encode(id))
            }
            Command::WorkspaceImport { force: false, .. } => "/v0/workspace/import".to_string(),
            Command::WorkspaceImport { force: true, .. } => {
                "/v0/workspace/import?force=true".to_string()
            }
            Command::WorkspaceExport { .. } => "/v0/workspace/export".to_string(),
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
            Command::ExportAuditJsonl { .. } => "/v0/audit/export".to_string(),
            Command::ExportRunAudit { .. } => "/v0/runs/export".to_string(),
            Command::ExportSerialLog { .. } => "/v0/serial/export".to_string(),
            Command::LlmSet { .. } => "/v0/llm/config".to_string(),
            Command::LlmClear => "/v0/llm/config/clear".to_string(),
            Command::LlmLoadKey { .. } => "/v0/llm/stored-key/load".to_string(),
            Command::QemuPath { .. } => "/v0/qemu/path".to_string(),
            Command::QemuClear => "/v0/qemu/path/clear".to_string(),
            Command::QemuDownload | Command::QemuStatus => "/v0/qemu/download".to_string(),
            Command::QemuCancel => "/v0/qemu/download/cancel".to_string(),
            Command::ToolchainDownload => "/v0/toolchain/download".to_string(),
            Command::ToolchainCancel => "/v0/toolchain/download/cancel".to_string(),
            Command::ToolchainPath { .. } => "/v0/toolchain/path".to_string(),
            Command::ToolchainClear => "/v0/toolchain/path/clear".to_string(),
            Command::PreflightRun => "/v0/preflight/run".to_string(),
            Command::PreflightAck => "/v0/preflight/ack".to_string(),
            Command::AuditAlertSet { .. } => "/v0/audit/alert".to_string(),
            Command::ThemeSet { .. } => "/v0/settings/theme".to_string(),
            Command::LanguageSet { .. } => "/v0/settings/language".to_string(),
        }
    }

    /// The path an export writes to, when this command is one.
    pub fn output_path(&self) -> Option<&str> {
        match self {
            Command::ExportAuditJsonl { out }
            | Command::ExportRunAudit { out, .. }
            | Command::ExportSerialLog { out } => Some(out),
            _ => None,
        }
    }

    /// The JSON body, or `None` for the commands that take none.
    pub fn body(&self) -> Option<serde_json::Value> {
        match self {
            Command::Run { task, sandbox } => match sandbox {
                // The declaration rides in the body: a run is a request, and the
                // sandbox it wants is part of it (v0.9 sandbox F2d).
                Some(name) => Some(json!({ "user_input": task, "sandbox": name })),
                None => Some(json!({ "user_input": task })),
            },
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
            Command::ExportAuditJsonl { out } | Command::ExportSerialLog { out } => {
                Some(json!({ "path": out }))
            }
            Command::ExportRunAudit { run_id, out } => {
                Some(json!({ "run_id": run_id, "path": out }))
            }
            // `lib::run` reads `--api-key-file` before this is called, so the key
            // is inline by the time any body is built.
            Command::LlmSet {
                api_key,
                base_url,
                model,
                provider_id,
                remember,
                ..
            } => {
                let mut body = json!({
                    "api_key": api_key.clone().unwrap_or_default(),
                    "base_url": base_url,
                    "model": model,
                });
                if let Some(provider_id) = provider_id {
                    body["provider_id"] = json!(provider_id);
                }
                if *remember {
                    body["remember"] = json!(true);
                }
                Some(body)
            }
            Command::LlmLoadKey { provider_id } => Some(json!({ "provider_id": provider_id })),
            Command::SandboxesSwitch { name } => Some(json!({ "name": name })),
            // The two decisions take no body: the id is in the path, and who
            // decided is the caller's own identity.
            Command::SandboxesRequestsApprove { .. } | Command::SandboxesRequestsReject { .. } => {
                Some(json!({}))
            }
            // Project in/out: the import sends the archive itself (the CLI reads the
            // file and posts the bytes, outside the JSON body path), the export
            // sends a bodyless `POST` like the other controls that answer themselves.
            Command::WorkspaceImport { .. } => None,
            Command::WorkspaceExport { .. } => Some(json!({})),
            Command::QemuPath { path } | Command::ToolchainPath { path } => {
                Some(json!({ "path": path }))
            }
            Command::AuditAlertSet { enabled } => Some(json!({ "enabled": enabled })),
            Command::ThemeSet { theme } => Some(json!({ "theme": theme })),
            Command::LanguageSet { language } => Some(json!({ "language": language })),
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
            // A switch stops the running VM too, and refuses while a run is in
            // flight — same family as `vm stop`.
            Command::SandboxesSwitch { name } => Some(format!(
                "Switch this node's sandbox to {name:?}? The running VM is stopped and started again."
            )),
            Command::SnapshotsDelete { name } => Some(format!("Delete snapshot {name:?}?")),
            // The two decisions are permission actions: approving lets someone
            // else's change happen, so it asks the same way a switch does.
            Command::SandboxesRequestsApprove { id } => {
                Some(format!("Approve sandbox request {id:?}?"))
            }
            Command::SandboxesRequestsReject { id } => {
                Some(format!("Reject sandbox request {id:?}?"))
            }
            Command::SessionsDelete { session_id } => {
                Some(format!("Delete session {session_id:?}?"))
            }
            Command::SessionsClearAll => Some("Delete every session?".to_string()),
            // Losing the configuration means finding the values again, and the
            // API key cannot be read back out of the host at all.
            Command::LlmClear => Some("Forget the model configuration?".to_string()),
            Command::QemuClear => Some("Forget the configured QEMU path?".to_string()),
            Command::ToolchainClear => Some("Forget the configured toolchain path?".to_string()),
            _ => None,
        }
    }
}

/// The default `--limit` for `audit events`, which the endpoint requires.
pub const DEFAULT_EVENT_LIMIT: usize = 20;

/// The default file an `export audit-jsonl` writes (server-side, inside the
/// workspace).
pub const DEFAULT_AUDIT_EXPORT: &str = "audit.jsonl";

/// The default file an `export serial-log` writes.
pub const DEFAULT_SERIAL_EXPORT: &str = "serial.log";

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
    /// `--wait`: `toolchain download` / `preflight run` print progress until the
    /// work finishes.
    pub wait: bool,
}

/// The result of parsing: a command, or one of the two informational flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Help,
    Version,
    /// Boxed because `Args` is much larger than the other two variants, and one
    /// `Parsed` exists per process.
    Command(Box<Args>),
}

/// The options `parse` collects, so `parse_command` can reach them without a
/// parameter per flag.
#[derive(Debug, Default, Clone)]
struct Flags {
    limit: Option<usize>,
    out: Option<String>,
    /// `--status <s>`: the request queue's filter (v0.9 sandbox F2c).
    status: Option<String>,
    /// `--force`: an import may replace what is already in the workspace.
    force: bool,
    /// `--sandbox <name>`: run under this definition (v0.9 sandbox F2d).
    sandbox: Option<String>,
    api_key: Option<String>,
    api_key_file: Option<PathBuf>,
    base_url: Option<String>,
    model: Option<String>,
    provider_id: Option<String>,
    remember: bool,
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
    let mut wait = false;
    let mut words: Vec<String> = Vec::new();
    let mut flags = Flags::default();

    let mut args = argv.into_iter();
    while let Some(flag) = args.next() {
        let mut value = |flag: &str| args.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--version" | "-V" => return Ok(Parsed::Version),
            "--json" => json = true,
            "--yes" | "-y" => yes = true,
            "--follow" | "-f" => follow = true,
            "--wait" | "-w" => wait = true,
            "--remember" => flags.remember = true,
            "--remote" => remote = Some(value("--remote")?),
            "--data-dir" => data_dir = Some(PathBuf::from(value("--data-dir")?)),
            "--workspace" => workspace = Some(PathBuf::from(value("--workspace")?)),
            "--token-file" => token_file = Some(PathBuf::from(value("--token-file")?)),
            "--token" => token = Some(value("--token")?),
            "--out" => flags.out = Some(value("--out")?),
            "--status" => flags.status = Some(value("--status")?),
            "--force" => flags.force = true,
            "--sandbox" => flags.sandbox = Some(value("--sandbox")?),
            "--api-key" => flags.api_key = Some(value("--api-key")?),
            "--api-key-file" => flags.api_key_file = Some(PathBuf::from(value("--api-key-file")?)),
            "--base-url" => flags.base_url = Some(value("--base-url")?),
            "--model" => flags.model = Some(value("--model")?),
            "--provider-id" => flags.provider_id = Some(value("--provider-id")?),
            "--limit" => {
                let raw = value("--limit")?;
                flags.limit = Some(
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

    let command = parse_command(&words, &flags)?;
    if follow && !matches!(command, Command::Run { .. }) {
        return Err("--follow is only meaningful for `run`".to_string());
    }
    if wait
        && !matches!(
            command,
            Command::ToolchainDownload | Command::PreflightRun | Command::QemuDownload
        )
    {
        return Err(
            "--wait is only meaningful for `toolchain download`, `preflight run` and \
             `qemu download`"
                .to_string(),
        );
    }
    Ok(Parsed::Command(Box::new(Args {
        command,
        remote,
        data_dir,
        workspace: workspace.unwrap_or_else(|| PathBuf::from(".")),
        token_file,
        token,
        json,
        yes,
        follow,
        wait,
    })))
}

fn parse_command(words: &[String], flags: &Flags) -> Result<Command, String> {
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
        (Some("runs"), Some("list"), None, None) => Some(Command::RunsList { limit: flags.limit }),
        (Some("runs"), Some("get"), Some(run_id), None) => Some(Command::RunsGet {
            run_id: run_id.to_string(),
        }),
        (Some("audit"), Some("status"), None, None) => Some(Command::AuditStatus),
        (Some("audit"), Some("events"), None, None) => Some(Command::AuditEvents {
            limit: flags.limit.unwrap_or(DEFAULT_EVENT_LIMIT),
        }),
        (Some("snapshots"), Some("list"), None, None) => Some(Command::SnapshotsList),
        (Some("sandboxes"), Some("list"), None, None) => Some(Command::SandboxesList),
        (Some("sandboxes"), Some("current"), None, None) => Some(Command::SandboxesCurrent),
        (Some("sandboxes"), Some("candidates"), None, None) => Some(Command::SandboxesCandidates),
        (Some("sandboxes"), Some("show"), Some(name), None) => Some(Command::SandboxesShow {
            name: name.to_string(),
        }),
        (Some("sandboxes"), Some("switch"), Some(name), None) => Some(Command::SandboxesSwitch {
            name: name.to_string(),
        }),
        // The request queue (v0.9 sandbox F2c). `requests approve <id>` is exactly
        // the four words the parser reaches; the filter is a flag, not a word, so
        // it can sit anywhere the other flags may.
        (Some("sandboxes"), Some("requests"), None, None) => Some(Command::SandboxesRequests {
            status: flags.status.clone(),
        }),
        (Some("sandboxes"), Some("requests"), Some("approve"), Some(id)) => {
            Some(Command::SandboxesRequestsApprove { id: id.to_string() })
        }
        (Some("sandboxes"), Some("requests"), Some("reject"), Some(id)) => {
            Some(Command::SandboxesRequestsReject { id: id.to_string() })
        }
        // Project in/out (v0.9). `workspace import <archive>` is three words,
        // `workspace export` two; the flags (`--force`, `--out`) may sit anywhere.
        (Some("workspace"), Some("import"), Some(archive), None) => {
            Some(Command::WorkspaceImport {
                archive: PathBuf::from(archive),
                force: flags.force,
            })
        }
        (Some("workspace"), Some("export"), None, None) => Some(Command::WorkspaceExport {
            out: flags.out.clone().map(PathBuf::from),
        }),
        (Some("run"), Some(task), None, None) => Some(Command::Run {
            task: task.to_string(),
            sandbox: flags.sandbox.clone(),
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
        // ---- export ----
        (Some("export"), Some("audit-jsonl"), None, None) => Some(Command::ExportAuditJsonl {
            out: default_out(flags, DEFAULT_AUDIT_EXPORT),
        }),
        (Some("export"), Some("serial-log"), None, None) => Some(Command::ExportSerialLog {
            out: default_out(flags, DEFAULT_SERIAL_EXPORT),
        }),
        (Some("export"), Some("run-audit"), Some(run_id), None) => Some(Command::ExportRunAudit {
            run_id: run_id.to_string(),
            out: default_out(flags, &format!("run-{run_id}.jsonl")),
        }),
        // ---- configuration ----
        (Some("llm"), Some("set"), None, None) => Some(llm_set(flags)?),
        (Some("llm"), Some("clear"), None, None) => Some(Command::LlmClear),
        (Some("llm"), Some("load-key"), Some(provider_id), None) => Some(Command::LlmLoadKey {
            provider_id: provider_id.to_string(),
        }),
        (Some("qemu"), Some("path"), Some(path), None) => Some(Command::QemuPath {
            path: path.to_string(),
        }),
        (Some("qemu"), Some("clear"), None, None) => Some(Command::QemuClear),
        (Some("qemu"), Some("download"), None, None) => Some(Command::QemuDownload),
        (Some("qemu"), Some("cancel"), None, None) => Some(Command::QemuCancel),
        (Some("qemu"), Some("status"), None, None) => Some(Command::QemuStatus),
        (Some("toolchain"), Some("download"), None, None) => Some(Command::ToolchainDownload),
        (Some("toolchain"), Some("cancel"), None, None) => Some(Command::ToolchainCancel),
        (Some("toolchain"), Some("path"), Some(path), None) => Some(Command::ToolchainPath {
            path: path.to_string(),
        }),
        (Some("toolchain"), Some("clear"), None, None) => Some(Command::ToolchainClear),
        (Some("preflight"), Some("run"), None, None) => Some(Command::PreflightRun),
        (Some("preflight"), Some("ack"), None, None) => Some(Command::PreflightAck),
        (Some("audit"), Some("alert"), Some("set"), Some(on_off)) => Some(Command::AuditAlertSet {
            enabled: parse_on_off(on_off)?,
        }),
        (Some("theme"), Some("set"), Some(theme), None) => Some(Command::ThemeSet {
            theme: theme.to_string(),
        }),
        (Some("language"), Some("set"), Some(language), None) => Some(Command::LanguageSet {
            language: language.to_string(),
        }),
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
        (Some("export"), Some("run-audit"), None) => {
            Err("export run-audit needs a <run_id>".to_string())
        }
        (Some("llm"), Some("load-key"), None) => {
            Err("llm load-key needs a <provider_id>".to_string())
        }
        (Some("qemu" | "toolchain"), Some("path"), None) => {
            Err(format!("{} path needs a <file>", w0.unwrap_or_default()))
        }
        (Some("qemu"), Some(other), _) => Err(format!("unknown qemu subcommand {other:?}")),
        (Some("sandboxes"), Some("show"), None) => Err("sandboxes show needs a <name>".to_string()),
        (Some("sandboxes"), Some("switch"), None) => {
            Err("sandboxes switch needs a <name>".to_string())
        }
        (Some("sandboxes"), Some("requests"), Some("approve" | "reject")) => {
            Err("sandboxes requests approve|reject needs an <id>".to_string())
        }
        (Some("sandboxes"), Some("requests"), Some(other)) => {
            Err(format!("unknown requests subcommand {other:?}"))
        }
        (Some("sandboxes"), Some(other), _) => {
            Err(format!("unknown sandboxes subcommand {other:?}"))
        }
        (Some("workspace"), Some("import"), None) => {
            Err("workspace import needs an <archive>".to_string())
        }
        (Some("workspace"), Some(other), _) => {
            Err(format!("unknown workspace subcommand {other:?}"))
        }
        (Some("audit"), Some("alert"), Some("set")) => {
            Err("audit alert set needs on|off".to_string())
        }
        (Some("audit"), Some("alert"), Some(other)) => {
            Err(format!("unknown audit alert subcommand {other:?}"))
        }
        (Some("theme"), Some("set"), None) => Err("theme set needs light|dark|system".to_string()),
        (Some("language"), Some("set"), None) => Err("language set needs system|en|zh".to_string()),
        (Some("llm"), Some("set"), Some(other)) => Err(format!("unknown llm subcommand {other:?}")),
        (Some("run" | "snapshots" | "sessions"), Some(subcommand), None)
            if subcommand != "list" =>
        {
            Err(format!("{subcommand:?} needs an argument"))
        }
        (Some(command), Some(other), _) => Err(format!("unknown or incomplete: {command} {other}")),
        (Some(command), None, _) => Err(format!("unknown command {command:?}")),
    }
}

/// `--out`, or the command's default: a relative name the *server* resolves
/// against the workspace root.
fn default_out(flags: &Flags, fallback: &str) -> String {
    flags.out.clone().unwrap_or_else(|| fallback.to_string())
}

/// `llm set`: the three required fields plus the two optional ones.
///
/// The endpoint requires `api_key`, `base_url` and `model` to be *present* (an
/// empty value reads as missing), so the CLI asks for all three; `provider_id`
/// and `remember` are optional there and here.
fn llm_set(flags: &Flags) -> Result<Command, String> {
    if flags.api_key.is_some() && flags.api_key_file.is_some() {
        return Err("llm set: use one of --api-key or --api-key-file, not both".to_string());
    }
    if flags.api_key.is_none() && flags.api_key_file.is_none() {
        return Err("llm set needs --api-key <key> or --api-key-file <path>".to_string());
    }
    let base_url = flags
        .base_url
        .clone()
        .ok_or_else(|| "llm set needs --base-url <url>".to_string())?;
    let model = flags
        .model
        .clone()
        .ok_or_else(|| "llm set needs --model <model>".to_string())?;
    Ok(Command::LlmSet {
        api_key: flags.api_key.clone(),
        api_key_file: flags.api_key_file.clone(),
        base_url,
        model,
        provider_id: flags.provider_id.clone(),
        remember: flags.remember,
    })
}

/// `on` / `off`, or the error to show.
fn parse_on_off(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "1" => Ok(true),
        "off" | "false" | "no" | "0" => Ok(false),
        other => Err(format!("audit alert set wants on or off, not {other:?}")),
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
            Parsed::Command(args) => *args,
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
                task: "say hi".to_string(),
                sandbox: None,
            }
        );
        // `--sandbox` rides along, in any position the other flags allow (F2d).
        assert_eq!(
            command(&["run", "say hi", "--sandbox", "blink"]),
            Command::Run {
                task: "say hi".to_string(),
                sandbox: Some("blink".to_string()),
            }
        );
        assert_eq!(
            command(&["--sandbox", "blink", "run", "say hi"]),
            Command::Run {
                task: "say hi".to_string(),
                sandbox: Some("blink".to_string()),
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
            Command::Run {
                task: "x".into(),
                sandbox: None
            }
            .request_path(),
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
        assert_eq!(
            Command::Run {
                task: "t".into(),
                sandbox: None
            }
            .method(),
            "POST"
        );
        assert_eq!(Command::VmStop.method(), "POST");
        assert_eq!(
            Command::Run {
                task: "t".into(),
                sandbox: None
            }
            .body(),
            Some(json!({ "user_input": "t" }))
        );
        // A declared sandbox joins the same body (v0.9 sandbox F2d).
        assert_eq!(
            Command::Run {
                task: "t".into(),
                sandbox: Some("blink".into())
            }
            .body(),
            Some(json!({ "user_input": "t", "sandbox": "blink" }))
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
        assert_eq!(
            Command::Run {
                task: "t".into(),
                sandbox: None
            }
            .confirmation(),
            None
        );
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
    fn the_export_and_configuration_commands_parse() {
        assert_eq!(
            command(&["export", "audit-jsonl"]),
            Command::ExportAuditJsonl {
                out: DEFAULT_AUDIT_EXPORT.to_string()
            }
        );
        assert_eq!(
            command(&["export", "audit-jsonl", "--out", "a.jsonl"]),
            Command::ExportAuditJsonl {
                out: "a.jsonl".to_string()
            }
        );
        assert_eq!(
            command(&["export", "serial-log"]),
            Command::ExportSerialLog {
                out: DEFAULT_SERIAL_EXPORT.to_string()
            }
        );
        assert_eq!(
            command(&["export", "run-audit", "r-1"]),
            Command::ExportRunAudit {
                run_id: "r-1".to_string(),
                out: "run-r-1.jsonl".to_string()
            }
        );
        assert_eq!(
            command(&["export", "run-audit", "r-1", "--out", "x.jsonl"]),
            Command::ExportRunAudit {
                run_id: "r-1".to_string(),
                out: "x.jsonl".to_string()
            }
        );
        assert_eq!(command(&["llm", "clear"]), Command::LlmClear);
        assert_eq!(
            command(&["llm", "load-key", "deepseek"]),
            Command::LlmLoadKey {
                provider_id: "deepseek".to_string()
            }
        );
        assert_eq!(
            command(&["qemu", "path", "C:/qemu/qemu-system-riscv64.exe"]),
            Command::QemuPath {
                path: "C:/qemu/qemu-system-riscv64.exe".to_string()
            }
        );
        assert_eq!(command(&["qemu", "clear"]), Command::QemuClear);
        assert_eq!(
            command(&["toolchain", "download"]),
            Command::ToolchainDownload
        );
        assert_eq!(command(&["toolchain", "cancel"]), Command::ToolchainCancel);
        assert_eq!(
            command(&["toolchain", "path", "/opt/gcc"]),
            Command::ToolchainPath {
                path: "/opt/gcc".to_string()
            }
        );
        assert_eq!(command(&["toolchain", "clear"]), Command::ToolchainClear);
        assert_eq!(command(&["preflight", "run"]), Command::PreflightRun);
        assert_eq!(command(&["preflight", "ack"]), Command::PreflightAck);
        assert_eq!(
            command(&["audit", "alert", "set", "on"]),
            Command::AuditAlertSet { enabled: true }
        );
        assert_eq!(
            command(&["audit", "alert", "set", "off"]),
            Command::AuditAlertSet { enabled: false }
        );
        assert_eq!(
            command(&["theme", "set", "dark"]),
            Command::ThemeSet {
                theme: "dark".to_string()
            }
        );
        assert_eq!(
            command(&["language", "set", "zh"]),
            Command::LanguageSet {
                language: "zh".to_string()
            }
        );
    }

    #[test]
    fn llm_set_takes_the_five_documented_fields() {
        assert_eq!(
            command(&[
                "llm",
                "set",
                "--api-key",
                "sk-not-real",
                "--base-url",
                "https://api.deepseek.com",
                "--model",
                "deepseek-chat",
            ]),
            Command::LlmSet {
                api_key: Some("sk-not-real".to_string()),
                api_key_file: None,
                base_url: "https://api.deepseek.com".to_string(),
                model: "deepseek-chat".to_string(),
                provider_id: None,
                remember: false,
            }
        );
        // The two optional fields, and the file as the other way to give the key.
        assert_eq!(
            command(&[
                "llm",
                "set",
                "--api-key-file",
                "/tmp/key",
                "--base-url",
                "u",
                "--model",
                "m",
                "--provider-id",
                "deepseek",
                "--remember",
            ]),
            Command::LlmSet {
                api_key: None,
                api_key_file: Some(PathBuf::from("/tmp/key")),
                base_url: "u".to_string(),
                model: "m".to_string(),
                provider_id: Some("deepseek".to_string()),
                remember: true,
            }
        );
    }

    #[test]
    fn llm_set_refuses_a_missing_or_doubled_key() {
        for args in [
            vec!["llm", "set"],
            vec!["llm", "set", "--api-key", "k"],
            vec!["llm", "set", "--api-key", "k", "--base-url", "u"],
            vec![
                "llm",
                "set",
                "--api-key",
                "k",
                "--api-key-file",
                "f",
                "--base-url",
                "u",
                "--model",
                "m",
            ],
        ] {
            assert!(parse_words(&args).is_err(), "{args:?} should not parse");
        }
    }

    #[test]
    fn wait_belongs_to_the_two_async_commands() {
        assert!(args_of(&["toolchain", "download", "--wait"]).wait);
        assert!(args_of(&["--wait", "preflight", "run"]).wait);
        assert!(!args_of(&["toolchain", "download"]).wait);
        for args in [
            vec!["health", "--wait"],
            vec!["preflight", "ack", "--wait"],
            vec!["run", "x", "--wait"],
        ] {
            let refused = parse_words(&args);
            assert!(refused.is_err(), "{args:?} should not parse");
            assert!(refused.expect_err("refused").contains("--wait"), "{args:?}");
        }
    }

    #[test]
    fn the_new_requests_match_the_api_table() {
        for (command, path) in [
            (
                Command::ExportAuditJsonl { out: "a".into() },
                "/v0/audit/export",
            ),
            (
                Command::ExportRunAudit {
                    run_id: "r".into(),
                    out: "a".into(),
                },
                "/v0/runs/export",
            ),
            (
                Command::ExportSerialLog { out: "a".into() },
                "/v0/serial/export",
            ),
            (Command::LlmClear, "/v0/llm/config/clear"),
            (
                Command::LlmLoadKey {
                    provider_id: "p".into(),
                },
                "/v0/llm/stored-key/load",
            ),
            (Command::QemuPath { path: "p".into() }, "/v0/qemu/path"),
            (Command::QemuClear, "/v0/qemu/path/clear"),
            (Command::ToolchainDownload, "/v0/toolchain/download"),
            (Command::ToolchainCancel, "/v0/toolchain/download/cancel"),
            (
                Command::ToolchainPath { path: "p".into() },
                "/v0/toolchain/path",
            ),
            (Command::ToolchainClear, "/v0/toolchain/path/clear"),
            (Command::PreflightRun, "/v0/preflight/run"),
            (Command::PreflightAck, "/v0/preflight/ack"),
            (Command::AuditAlertSet { enabled: true }, "/v0/audit/alert"),
            (
                Command::ThemeSet { theme: "d".into() },
                "/v0/settings/theme",
            ),
            (
                Command::LanguageSet {
                    language: "zh".into(),
                },
                "/v0/settings/language",
            ),
        ] {
            assert_eq!(command.request_path(), path, "{command:?}");
            assert_eq!(command.method(), "POST", "{command:?}");
        }
        assert_eq!(
            Command::LlmSet {
                api_key: Some("k".into()),
                api_key_file: None,
                base_url: "u".into(),
                model: "m".into(),
                provider_id: None,
                remember: false,
            }
            .request_path(),
            "/v0/llm/config"
        );
    }

    #[test]
    fn the_new_bodies_are_the_documented_objects() {
        assert_eq!(
            Command::ExportAuditJsonl {
                out: "a.jsonl".into()
            }
            .body(),
            Some(json!({ "path": "a.jsonl" }))
        );
        assert_eq!(
            Command::ExportRunAudit {
                run_id: "r".into(),
                out: "a.jsonl".into()
            }
            .body(),
            Some(json!({ "run_id": "r", "path": "a.jsonl" }))
        );
        assert_eq!(
            Command::ExportSerialLog {
                out: "s.log".into()
            }
            .body(),
            Some(json!({ "path": "s.log" }))
        );
        assert_eq!(
            Command::LlmLoadKey {
                provider_id: "p".into()
            }
            .body(),
            Some(json!({ "provider_id": "p" }))
        );
        assert_eq!(
            Command::QemuPath { path: "p".into() }.body(),
            Some(json!({ "path": "p" }))
        );
        assert_eq!(
            Command::AuditAlertSet { enabled: false }.body(),
            Some(json!({ "enabled": false }))
        );
        assert_eq!(
            Command::ThemeSet {
                theme: "dark".into()
            }
            .body(),
            Some(json!({ "theme": "dark" }))
        );
        assert_eq!(
            Command::LanguageSet {
                language: "zh".into()
            }
            .body(),
            Some(json!({ "language": "zh" }))
        );
        // The optional fields are left out when they were not given, and the key
        // is inline (the file has been read by the time a body is built).
        let minimal = Command::LlmSet {
            api_key: Some("k".into()),
            api_key_file: None,
            base_url: "u".into(),
            model: "m".into(),
            provider_id: None,
            remember: false,
        };
        assert_eq!(
            minimal.body(),
            Some(json!({ "api_key": "k", "base_url": "u", "model": "m" }))
        );
        assert_eq!(Command::ToolchainDownload.body(), None);
        assert_eq!(Command::ToolchainCancel.body(), None);
        assert_eq!(Command::PreflightRun.body(), None);
        assert_eq!(Command::PreflightAck.body(), None);
        assert_eq!(Command::LlmClear.body(), None);
    }

    #[test]
    fn the_clearing_commands_ask_first_and_nothing_else_does() {
        for command in [
            Command::LlmClear,
            Command::QemuClear,
            Command::ToolchainClear,
        ] {
            assert!(command.confirmation().is_some(), "{command:?}");
        }
        for command in [
            Command::ToolchainDownload,
            Command::ToolchainCancel,
            Command::PreflightRun,
            Command::PreflightAck,
            Command::ThemeSet { theme: "d".into() },
            Command::LanguageSet {
                language: "zh".into(),
            },
            Command::AuditAlertSet { enabled: true },
            Command::ExportAuditJsonl { out: "a".into() },
        ] {
            assert_eq!(command.confirmation(), None, "{command:?}");
        }
    }

    #[test]
    fn only_an_export_names_an_output_path() {
        assert_eq!(
            Command::ExportSerialLog {
                out: "s.log".into()
            }
            .output_path(),
            Some("s.log")
        );
        assert_eq!(
            Command::ExportRunAudit {
                run_id: "r".into(),
                out: "a".into()
            }
            .output_path(),
            Some("a")
        );
        assert_eq!(Command::Health.output_path(), None);
        assert_eq!(Command::ToolchainDownload.output_path(), None);
    }

    #[test]
    fn an_incomplete_configuration_command_is_refused() {
        for args in [
            vec!["export"],
            vec!["export", "run-audit"],
            vec!["export", "nope"],
            vec!["llm"],
            vec!["llm", "load-key"],
            vec!["llm", "nope"],
            vec!["qemu"],
            vec!["qemu", "path"],
            vec!["toolchain", "path"],
            vec!["toolchain", "nope"],
            vec!["preflight"],
            vec!["preflight", "nope"],
            vec!["audit", "alert"],
            vec!["audit", "alert", "set"],
            vec!["audit", "alert", "set", "maybe"],
            vec!["theme"],
            vec!["theme", "set"],
            vec!["language", "set"],
        ] {
            assert!(parse_words(&args).is_err(), "{args:?} should not parse");
        }
        // The server validates the vocabularies, so a value it will refuse is
        // still a well-formed command line here.
        assert_eq!(
            command(&["theme", "set", "mauve"]),
            Command::ThemeSet {
                theme: "mauve".to_string()
            }
        );
    }

    #[test]
    fn the_qemu_download_commands_parse() {
        assert_eq!(command(&["qemu", "download"]), Command::QemuDownload);
        assert_eq!(command(&["qemu", "cancel"]), Command::QemuCancel);
        assert_eq!(command(&["qemu", "status"]), Command::QemuStatus);
        // `status` is the one read-only member of the family, so it is the one GET.
        assert_eq!(Command::QemuStatus.method(), "GET");
        assert_eq!(Command::QemuDownload.method(), "POST");
        assert_eq!(Command::QemuCancel.method(), "POST");
        assert_eq!(Command::QemuDownload.request_path(), "/v0/qemu/download");
        assert_eq!(Command::QemuStatus.request_path(), "/v0/qemu/download");
        assert_eq!(
            Command::QemuCancel.request_path(),
            "/v0/qemu/download/cancel"
        );
        assert_eq!(Command::QemuDownload.body(), None);
        assert_eq!(Command::QemuCancel.body(), None);
        assert_eq!(Command::QemuStatus.body(), None);
        // Starting a download is not destructive, so nothing asks first (the
        // toolchain's `download` does not either); the clears still do.
        assert_eq!(Command::QemuDownload.confirmation(), None);
        assert_eq!(Command::QemuCancel.confirmation(), None);
        assert!(Command::QemuClear.confirmation().is_some());
    }

    #[test]
    fn wait_also_belongs_to_the_qemu_download() {
        assert!(args_of(&["qemu", "download", "--wait"]).wait);
        assert!(args_of(&["--wait", "qemu", "download"]).wait);
        assert!(!args_of(&["qemu", "download"]).wait);
        for args in [
            vec!["qemu", "status", "--wait"],
            vec!["qemu", "cancel", "--wait"],
        ] {
            let refused = parse_words(&args);
            assert!(refused.is_err(), "{args:?} should not parse");
            assert!(refused.expect_err("refused").contains("--wait"), "{args:?}");
        }
    }

    #[test]
    fn an_incomplete_qemu_command_is_refused() {
        for args in [
            vec!["qemu", "download", "now"],
            vec!["qemu", "nope"],
            vec!["qemu", "status", "extra"],
        ] {
            assert!(parse_words(&args).is_err(), "{args:?} should not parse");
        }
    }

    #[test]
    fn the_sandbox_commands_parse() {
        assert_eq!(command(&["sandboxes", "list"]), Command::SandboxesList);
        assert_eq!(
            command(&["sandboxes", "current"]),
            Command::SandboxesCurrent
        );
        assert_eq!(
            command(&["sandboxes", "candidates"]),
            Command::SandboxesCandidates
        );
        assert_eq!(
            command(&["sandboxes", "show", "blink"]),
            Command::SandboxesShow {
                name: "blink".to_string()
            }
        );

        // All four are read-only: `GET`, no body, nothing to confirm, no path.
        for candidate in [
            Command::SandboxesList,
            Command::SandboxesCurrent,
            Command::SandboxesCandidates,
            Command::SandboxesShow {
                name: "blink".to_string(),
            },
        ] {
            assert_eq!(candidate.method(), "GET");
            assert_eq!(candidate.body(), None);
            assert_eq!(candidate.confirmation(), None);
            assert_eq!(candidate.output_path(), None);
        }

        assert_eq!(Command::SandboxesList.request_path(), "/v0/sandboxes");
        assert_eq!(
            Command::SandboxesCurrent.request_path(),
            "/v0/sandboxes/current"
        );
        assert_eq!(
            Command::SandboxesCandidates.request_path(),
            "/v0/sandboxes/candidates"
        );
        assert_eq!(
            Command::SandboxesShow {
                name: "blink".to_string()
            }
            .request_path(),
            "/v0/sandboxes/blink"
        );
        // The switch is the family's one write: `POST`, a body, and a question.
        assert_eq!(
            command(&["sandboxes", "switch", "blink"]),
            Command::SandboxesSwitch {
                name: "blink".to_string()
            }
        );
        assert_eq!(
            Command::SandboxesSwitch {
                name: "blink".into()
            }
            .method(),
            "POST"
        );
        assert_eq!(
            Command::SandboxesSwitch {
                name: "blink".into()
            }
            .request_path(),
            "/v0/sandboxes/switch"
        );
        assert_eq!(
            Command::SandboxesSwitch {
                name: "blink".into()
            }
            .body(),
            Some(serde_json::json!({ "name": "blink" }))
        );
        assert!(Command::SandboxesSwitch {
            name: "blink".into()
        }
        .confirmation()
        .is_some());
        // A name is percent-encoded like a run id, so a slash cannot escape the
        // one segment the route matches on.
        assert_eq!(
            Command::SandboxesShow {
                name: "a/b".to_string()
            }
            .request_path(),
            "/v0/sandboxes/a%2Fb"
        );
    }

    #[test]
    fn the_request_commands_parse() {
        // The queue reads: `GET`, no body, nothing to confirm.
        assert_eq!(
            command(&["sandboxes", "requests"]),
            Command::SandboxesRequests { status: None }
        );
        assert_eq!(
            args_of(&["sandboxes", "requests", "--status", "pending"]).command,
            Command::SandboxesRequests {
                status: Some("pending".to_string())
            }
        );
        assert_eq!(
            Command::SandboxesRequests { status: None }.request_path(),
            "/v0/sandboxes/requests"
        );
        assert_eq!(
            Command::SandboxesRequests {
                status: Some("pending".to_string())
            }
            .request_path(),
            "/v0/sandboxes/requests?status=pending"
        );
        assert_eq!(Command::SandboxesRequests { status: None }.method(), "GET");
        assert_eq!(Command::SandboxesRequests { status: None }.body(), None);
        assert_eq!(
            Command::SandboxesRequests { status: None }.confirmation(),
            None
        );

        // The two decisions are `POST`s to the request's own path, with a body
        // (the actor's identity is the caller's, not a field) and a question.
        for (command, verb) in [
            (
                Command::SandboxesRequestsApprove {
                    id: "req-1-2".to_string(),
                },
                "approve",
            ),
            (
                Command::SandboxesRequestsReject {
                    id: "req-1-2".to_string(),
                },
                "reject",
            ),
        ] {
            assert_eq!(command.method(), "POST");
            assert_eq!(
                command.request_path(),
                format!("/v0/sandboxes/requests/req-1-2/{verb}")
            );
            assert_eq!(command.body(), Some(serde_json::json!({})));
            let question = command.confirmation().expect("a permission action asks");
            assert!(question.contains("req-1-2"), "{question}");
        }
        assert_eq!(
            command(&["sandboxes", "requests", "approve", "req-1-2"]),
            Command::SandboxesRequestsApprove {
                id: "req-1-2".to_string()
            }
        );
        assert_eq!(
            command(&["sandboxes", "requests", "reject", "req-1-2"]),
            Command::SandboxesRequestsReject {
                id: "req-1-2".to_string()
            }
        );
        // An id is percent-encoded like a name.
        assert_eq!(
            Command::SandboxesRequestsApprove {
                id: "a/b".to_string()
            }
            .request_path(),
            "/v0/sandboxes/requests/a%2Fb/approve"
        );
    }

    #[test]
    fn an_incomplete_sandboxes_command_is_refused() {
        for args in [
            vec!["sandboxes"],
            vec!["sandboxes", "show"],
            vec!["sandboxes", "switch"],
            vec!["sandboxes", "nope"],
            vec!["sandboxes", "list", "extra"],
            // The two decisions need their id, and a third word is not a verb.
            // (A fifth word is not examined at all — the parser reads four, which
            // is true of every command here, not just these.)
            vec!["sandboxes", "requests", "approve"],
            vec!["sandboxes", "requests", "reject"],
            vec!["sandboxes", "requests", "delete", "req-1-1"],
        ] {
            assert!(parse_words(&args).is_err(), "{args:?} should not parse");
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
