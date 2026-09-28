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
//! §3's model and no new credential), and then serves §6.2's roles: it **relays** a frame
//! down the destination's live session, answers **signalling** — where a `node_id` can be
//! reached, from the session it dialled in on and its entry — and answers **management** with
//! its registry: the node table and the room definitions, which a node merges with its own
//! files winning (§4.1's rule). It **stores no message**: it forwards a frame and forgets it.
//! What it does keep is transport state — who is dialled in, and §3.2's per-peer replay record.
//!
//! `peers.json` and `rooms.json` are the same files the other roles use, read from
//! `--data-dir`: the server is a peer (§6.4), so the nodes it knows are an ordinary entry
//! list, and the rooms it publishes are an ordinary room set. A data directory with neither is
//! a server that knows nobody and publishes nothing — which refuses every frame and hands out
//! an empty registry, honestly rather than conveniently.
//!
//! Its own key is `<data-dir>/node.key`, minted on the first start (§2: the first start with
//! networking configured) and printed as a fingerprint at startup. The deployer puts the
//! **public** half in the other nodes' `peers.json`, which is what §6.4 asks for and what
//! makes a node able to verify what the server signs.
//!
//! §7's audit aggregation is later still, and waits on M5's authorisation.

use net::{Listener, NodeKey, PeersFile, RelayServer, RoomsFile, TransportConfig, VersionedLoad};
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

    let rooms = match RoomsFile::load_in(&args.data_dir) {
        Ok(VersionedLoad::Missing) => RoomsFile::empty(),
        Ok(VersionedLoad::Current(file)) | Ok(VersionedLoad::Migrated { value: file, .. }) => file,
        Ok(VersionedLoad::TooNew { found }) => {
            return Err(format!(
                "rooms.json is version {found}; this build reads {}",
                RoomsFile::SCHEMA_VERSION
            ));
        }
        Err(error) => return Err(format!("rooms.json could not be read: {error}")),
    };
    let rooms_count = rooms.rooms.len();
    let rooms_path = args.data_dir.join(net::ROOMS_FILE);

    // §2: a key is minted on the first start that has networking configured, and never by a
    // read. A server deployment has networking, so this is that start.
    let key = NodeKey::load_or_create_in(&args.data_dir, true)
        .map_err(|error| format!("node.key could not be read: {error}"))?
        .ok_or("networking is configured, so a key should have been minted")?;

    let server = RelayServer::new(
        &args.node_id,
        key.clone(),
        peers,
        rooms,
        TransportConfig::default(),
    )
    .map_err(|error| format!("the data directory is not usable: {error}"))?;
    let listener = Listener::bind(args.bind.as_str())
        .map_err(|error| format!("{0} could not be bound: {error}", args.bind))?;
    let bound = listener
        .local_addr()
        .map_err(|error| format!("the bound address could not be read: {error}"))?;

    println!("riscdom-relay: a cross-region server, run by a deployer (not by the project)");
    println!("  node_id: {0}", args.node_id);
    println!("  binding: {bound}");
    println!("  key:     {0}", key.short_fingerprint());
    println!("  knowing: {known} peer(s) from {}", peers_path.display());
    println!(
        "  rooms:   {rooms_count} room(s) from {}",
        rooms_path.display()
    );
    println!("  roles:   relay, signalling (where a node is) and management (the registry)");
    println!(
        "  stores:  no message — a frame is forwarded down the destination's session and forgotten"
    );
    println!("  waits:   to be dialled; it never dials a node, so no hole punching is needed");
    println!("riscdom-relay: listening (Ctrl-C to stop)");

    server
        .serve(listener)
        .map_err(|error| format!("the listener failed: {error}"))
}
