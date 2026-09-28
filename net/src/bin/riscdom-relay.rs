//! The deployer's cross-region server (v1.0 M4d).
//!
//! ```text
//! riscdom-relay --data-dir <dir> --bind <addr> --node-id <name>
//! ```
//!
//! [connection.md §6.1](../../docs/connection.md) is why this is a program a **deployer**
//! runs and not a service: the repositories ship the software, and nothing here starts a
//! server, offers an endpoint, or points at one the project operates. There is no default
//! address — `--bind` is required, because naming one would be the project naming where a
//! server is.
//!
//! What it does: it **listens** and waits to be dialled (§6.3 — it never dials a node),
//! authenticates each frame's sender against its own `peers.json` (§6.3's authorisation is
//! §3's model and no new credential), and hands a frame down the destination's live
//! session. It **stores no message**: it forwards a frame and forgets it. What it does keep
//! is transport state — who is dialled in, and §3.2's per-peer replay record.
//!
//! `peers.json` is the same file the other roles use, read from `--data-dir`: the server is
//! a peer (§6.4), so the nodes it knows are an ordinary entry list. A data directory with
//! no `peers.json` is a server that knows nobody — and one that therefore refuses every
//! frame, which is honest rather than convenient.
//!
//! Signalling and management (the address query, and publishing the registry and room
//! definitions) land on this same server next; §7's audit aggregation is later still.

use net::{Listener, PeersFile, RelayServer, TransportConfig, VersionedLoad};
use std::path::PathBuf;
use std::process::ExitCode;

/// What the deployer asked for.
struct Args {
    data_dir: PathBuf,
    bind: String,
    node_id: String,
}

const USAGE: &str = "usage: riscdom-relay --data-dir <dir> --bind <addr> --node-id <name>";

fn usage_error(why: &str) -> ExitCode {
    eprintln!("riscdom-relay: {why}");
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut data_dir: Option<PathBuf> = None;
    let mut bind: Option<String> = None;
    let mut node_id: Option<String> = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--data-dir" => {
                let value = rest.next().ok_or("--data-dir needs a directory")?;
                data_dir = Some(PathBuf::from(value));
            }
            "--bind" => {
                let value = rest.next().ok_or("--bind needs an address")?;
                bind = Some(value.clone());
            }
            "--node-id" => {
                let value = rest.next().ok_or("--node-id needs a name")?;
                node_id = Some(value.clone());
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        data_dir: data_dir.ok_or("--data-dir is required")?,
        bind: bind.ok_or("--bind is required")?,
        node_id: node_id.ok_or("--node-id is required")?,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let args = match parse_args(&args) {
        Ok(args) => args,
        Err(why) => return usage_error(&why),
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("riscdom-relay: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), String> {
    let peers = match PeersFile::load_in(&args.data_dir) {
        Ok(VersionedLoad::Missing) => PeersFile::empty(),
        Ok(VersionedLoad::Current(file)) | Ok(VersionedLoad::Migrated { value: file, .. }) => file,
        Ok(VersionedLoad::TooNew { found }) => {
            return Err(format!(
                "peers.json is version {found}; this build reads {}",
                PeersFile::SCHEMA_VERSION
            ));
        }
        Err(error) => return Err(format!("peers.json could not be read: {error}")),
    };
    let known = peers.peers.len();
    let peers_path = args.data_dir.join(net::PEERS_FILE);

    let server = RelayServer::new(&args.node_id, peers, TransportConfig::default())
        .map_err(|error| format!("the peer table is not usable: {error}"))?;
    let listener = Listener::bind(args.bind.as_str())
        .map_err(|error| format!("{0} could not be bound: {error}", args.bind))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("the bound address could not be read: {error}"))?;

    println!("riscdom-relay: a cross-region server, run by a deployer (not by the project)");
    println!("  node_id: {0}", args.node_id);
    println!("  binding: {bound}");
    println!("  knowing: {known} peer(s) from {}", peers_path.display());
    println!(
        "  stores:  no message — a frame is forwarded down the destination's session and forgotten"
    );
    println!("  waits:   to be dialled; it never dials a node, so no hole punching is needed");
    println!("riscdom-relay: listening (Ctrl-C to stop)");

    server
        .serve(listener)
        .map_err(|error| format!("the listener failed: {error}"))
}
