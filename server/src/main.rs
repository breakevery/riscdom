//! `riscdom-server`: start the control plane.
//!
//! ```text
//! riscdom-server [--bind <addr>] [--workspace <dir>] [--data-dir <dir>] [--heartbeat-ms <n>]
//! ```
//!
//! Exit codes: `0` after a clean stop, `1` when the workspace or the bind fails,
//! `2` on a usage error.

use host::AppState;
use server::{Server, ServerConfig, DEFAULT_BIND, DEFAULT_HEARTBEAT_MS};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

const USAGE: &str = "\
usage: riscdom-server [--bind <addr>] [--workspace <dir>] [--data-dir <dir>] [--heartbeat-ms <n>]

  --bind          address to listen on (default 127.0.0.1:7821, or $RISCDOM_BIND)
  --workspace     the workspace this host owns (default: the current directory);
                  its audit chain lives at <workspace>/.riscdom/audit.db
  --data-dir      where settings and sessions live (default: this platform's host data dir)
  --heartbeat-ms  SSE heartbeat period, 0 disables it (default 15000)
";

/// The command line.
struct Cli {
    bind: SocketAddr,
    workspace: PathBuf,
    data_dir: Option<PathBuf>,
    heartbeat: Option<Duration>,
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

impl Cli {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut bind_raw =
            std::env::var("RISCDOM_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
        let mut workspace: Option<PathBuf> = None;
        let mut data_dir: Option<PathBuf> = None;
        let mut heartbeat_ms = DEFAULT_HEARTBEAT_MS;

        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--bind" => bind_raw = next_value(&mut args, &flag)?,
                "--workspace" => workspace = Some(PathBuf::from(next_value(&mut args, &flag)?)),
                "--data-dir" => data_dir = Some(PathBuf::from(next_value(&mut args, &flag)?)),
                "--heartbeat-ms" => {
                    let raw = next_value(&mut args, &flag)?;
                    heartbeat_ms = raw
                        .parse()
                        .map_err(|e| format!("--heartbeat-ms {raw:?} is not a number: {e}"))?;
                }
                other => return Err(format!("unknown argument {other:?}")),
            }
        }

        let bind = bind_raw
            .parse()
            .map_err(|e| format!("--bind {bind_raw:?} is not an address: {e}"))?;
        let heartbeat = if heartbeat_ms == 0 {
            None
        } else {
            Some(Duration::from_millis(heartbeat_ms))
        };
        Ok(Self {
            bind,
            workspace: workspace.unwrap_or_else(|| PathBuf::from(".")),
            data_dir,
            heartbeat,
        })
    }
}

/// Open the host the control plane fronts.
///
/// `--data-dir` injects the instance's data directory (the v0.8 multi-agent
/// precondition); without it the platform default applies, as in the desktop app.
fn build_state(cli: &Cli) -> Result<Arc<AppState>, String> {
    let state = match &cli.data_dir {
        Some(data_dir) => AppState::with_data_dir(cli.workspace.clone(), data_dir.clone()),
        None => AppState::new(cli.workspace.clone()),
    };
    state.map(Arc::new).map_err(|e| e.to_string())
}

async fn run(cli: Cli, app: Arc<AppState>) {
    let config = ServerConfig::new(cli.bind).with_heartbeat(cli.heartbeat);
    let server = Server::new(app, config);
    match server.start().await {
        Ok(running) => {
            println!(
                "riscdom-server {} listening on http://{}",
                server::VERSION,
                running.local_addr()
            );
            println!("  GET /v0/health   GET /v0/status   GET /v0/events (SSE)");
            println!("  stop the server with Ctrl+C");
            std::future::pending::<()>().await;
        }
        Err(e) => {
            eprintln!("riscdom-server: cannot bind {}: {e}", cli.bind);
            std::process::exit(1);
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let cli = match Cli::parse(args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("riscdom-server: {message}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let app = match build_state(&cli) {
        Ok(app) => app,
        Err(message) => {
            eprintln!("riscdom-server: cannot open the workspace state: {message}");
            return ExitCode::from(1);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("riscdom-server: cannot start the async runtime: {e}");
            return ExitCode::from(1);
        }
    };
    runtime.block_on(run(cli, app));
    ExitCode::SUCCESS
}
