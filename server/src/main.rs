//! `riscdom-server`: start the control plane.
//!
//! ```text
//! riscdom-server [--bind <addr>] [--workspace <dir>] [--data-dir <dir>]
//!                [--heartbeat-ms <n>] [--auth | --no-auth]
//! ```
//!
//! Exit codes: `0` after a clean stop, `1` when the workspace, the token or the
//! bind fails, `2` on a usage error.

use host_core::AppState;
use server::{cli, token, AuthMode, Cli, NoAuth, Server, ServerConfig, TokenAuth};
use std::process::ExitCode;
use std::sync::Arc;

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

/// Build the authentication hook the server will install.
///
/// The token is never printed — not here, not in an error, not in a log line. The
/// path it lives at is printed, because the operator needs to know where to read
/// it from.
fn build_auth(cli: &Cli, app: &AppState) -> Result<Arc<dyn server::Authn>, String> {
    match cli.auth {
        AuthMode::None => {
            eprintln!(
                "riscdom-server: WARNING --no-auth is on: anyone who can reach {} can use every \
                 endpoint, including the destructive ones (delete a session, stop the VM, change \
                 the LLM configuration)",
                cli.bind
            );
            Ok(Arc::new(NoAuth))
        }
        AuthMode::Token => {
            let file = token::load_or_create(app.data_dir()).map_err(|e| e.to_string())?;
            println!(
                "riscdom-server: bearer token required; the token file is {}",
                file.path().display()
            );
            if file.was_created() {
                println!(
                    "riscdom-server: a new token was generated there; it is never printed or \
                     logged, so read it from the file"
                );
            }
            Ok(Arc::new(TokenAuth::new(file.token())))
        }
    }
}

async fn run(cli: Cli, app: Arc<AppState>) {
    let authn = match build_auth(&cli, &app) {
        Ok(authn) => authn,
        Err(message) => {
            eprintln!("riscdom-server: {message}");
            std::process::exit(1);
        }
    };
    let config = ServerConfig::new(cli.bind)
        .with_heartbeat(cli.heartbeat)
        .with_authn(authn)
        .with_log_level(cli.log_level);
    let server = Server::new(app, config);
    match server.start().await {
        Ok(running) => {
            println!(
                "riscdom-server {} listening on http://{}",
                server::VERSION,
                running.local_addr()
            );
            println!("  GET  /v0/health   /v0/status   /v0/events (SSE)");
            println!("  POST /v0/…  the control endpoints of docs/control-plane-api.md");
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
        print!("{}", cli::USAGE);
        return ExitCode::SUCCESS;
    }
    let cli = match Cli::parse(args) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("riscdom-server: {message}");
            eprint!("{}", cli::USAGE);
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
