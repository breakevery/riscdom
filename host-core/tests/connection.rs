//! The connection layer's files, as the host loads them (v1.0 batch W).
//!
//! [connection.md §2](../../docs/connection.md) gives a node its key on the first start with
//! networking configured; §4 and §5 put the peer table and the rooms beside it. These check the
//! host's half of that: **when** the three are read (the network settings are the switch),
//! what a refusal does (nothing is written over a newer file, and the refusal is visible), and
//! what the host says about it (two event names, and a problem a caller can read).

use host_core::settings::{LocalSettings, NetworkSettings, SETTINGS_VERSION};
use host_core::state::AppState;
use host_core::{ConnectionFile, EventFilter};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "riscdom-connection-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// A data directory whose settings **configure the network** — the switch the loader reads.
///
/// Nothing else decides anything: `settings.network.is_some()` is the whole condition
/// (connection.md §2 does not give a node a key until it is on a network).
fn configured(tag: &str) -> PathBuf {
    let dir = unique_dir(tag);
    let settings = LocalSettings {
        version: SETTINGS_VERSION,
        network: Some(NetworkSettings {
            lan_enabled: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    settings
        .save(&dir.join("settings.json"))
        .expect("settings.json");
    dir
}

/// A data directory whose settings configure the network **and name a cross-region server** (v1.0 V-2).
fn configured_pointing(tag: &str, server_node_id: &str) -> PathBuf {
    let dir = unique_dir(tag);
    let settings = LocalSettings {
        version: SETTINGS_VERSION,
        network: Some(NetworkSettings {
            lan_enabled: true,
            cross_region_server: Some(server_node_id.to_string()),
            ..Default::default()
        }),
        ..Default::default()
    };
    settings
        .save(&dir.join("settings.json"))
        .expect("settings.json");
    dir
}

/// Every action this instance's chain carries.
fn actions(state: &AppState) -> Vec<String> {
    state
        .list_events(200, EventFilter::default())
        .expect("events")
        .into_iter()
        .map(|event| event.action)
        .collect()
}

/// The detail of the one event with this action.
fn detail_of(state: &AppState, action: &str) -> serde_json::Value {
    state
        .list_events(200, EventFilter::default())
        .expect("events")
        .into_iter()
        .find(|event| event.action == action)
        .unwrap_or_else(|| panic!("no {action} row: {:?}", actions(state)))
        .detail
}

/// State over an injected workspace *and* data directory. The two are separate on purpose: the
/// audit chain lives under the workspace (§AppState::with_data_dir), so two states over one
/// workspace are a restart, and two data directories in one process stay apart.
fn state_in(workspace: &Path, data_dir: &Path) -> AppState {
    AppState::with_data_dir(workspace, data_dir).expect("state")
}

#[test]
fn a_node_with_no_network_configured_reads_nothing_and_grows_no_key() {
    let data_dir = unique_dir("unconfigured");
    let state = state_in(&unique_dir("unconfigured-ws"), &data_dir);

    assert!(state.node_key().is_none());
    assert!(state.peers().is_none());
    assert!(state.rooms().is_none());
    assert!(state.connection_problem().is_none());
    // Nothing was created, and nothing was said: an unconfigured node is not an event.
    assert!(!data_dir.join("node.key").exists());
    assert!(!actions(&state)
        .iter()
        .any(|action| action.starts_with("host.connection.")));
}

#[test]
fn a_configured_node_mints_its_key_once_and_names_it() {
    let data_dir = configured("mint");
    let workspace = unique_dir("mint-ws");
    let state = state_in(&workspace, &data_dir);

    let key = state
        .node_key()
        .expect("a key on the first configured start");
    let key_path = data_dir.join("node.key");
    assert!(key_path.is_file(), "it was written");
    assert!(std::fs::metadata(&key_path).expect("metadata").len() > 0);

    let detail = detail_of(&state, "host.connection.key_minted");
    assert_eq!(detail["node_id"], agent::device());
    assert_eq!(detail["fingerprint"], key.short_fingerprint());

    let minted = |state: &AppState| {
        actions(state)
            .iter()
            .filter(|action| *action == "host.connection.key_minted")
            .count()
    };
    assert_eq!(minted(&state), 1);

    // A restart reads the key it wrote — the same workspace, so the same chain — and mints
    // nothing a second time.
    let restarted = state_in(&workspace, &data_dir);
    assert_eq!(
        restarted
            .node_key()
            .expect("the key comes back")
            .fingerprint(),
        key.fingerprint()
    );
    assert_eq!(
        minted(&restarted),
        1,
        "a node that already has a key does not mint one again"
    );
}

#[test]
fn a_peer_table_and_a_room_file_that_load_are_handed_out() {
    let data_dir = configured("files");
    // Written by `net`'s own writers, so the host reads what a deployer's files would be.
    let mut peers = net::PeersFile::empty();
    let other = net::NodeKey::generate().expect("key");
    peers.peers.push(net::PeerEntry::new(
        "dev-b",
        "127.0.0.1:2",
        other.public_jwk(),
    ));
    net::PeersFile::save_in(&data_dir, &peers).expect("peers.json");

    let mut rooms = net::RoomsFile::empty();
    rooms.rooms.push(net::Room {
        name: "lab".to_string(),
        members: vec!["dev-b".to_string()],
        rules: net::RoomRules::new(net::RateRule {
            messages: 10,
            window_seconds: 60,
        }),
    });
    net::RoomsFile::save_in(&data_dir, &rooms).expect("rooms.json");

    let state = state_in(&unique_dir("files-ws"), &data_dir);
    assert_eq!(state.peers().expect("the peer table"), peers);
    assert_eq!(state.rooms().expect("the room definitions"), rooms);
    assert!(state.connection_problem().is_none());
}

#[test]
fn a_too_new_peer_table_is_refused_named_and_never_overwritten() {
    let data_dir = configured("too-new-peers");
    let path = data_dir.join("peers.json");
    let original = r#"{"schema_version":2,"peers":[]}"#;
    std::fs::write(&path, original).expect("write");

    let state = state_in(&unique_dir("too-new-peers-ws"), &data_dir);

    assert!(state.peers().is_none(), "a newer file is refused, not read");
    assert_eq!(std::fs::read_to_string(&path).expect("read"), original);
    let problem = state.connection_problem().expect("a reported problem");
    assert!(problem.contains("peers.json"), "{problem}");
    assert!(problem.contains('2'), "{problem}");

    let detail = detail_of(&state, "host.connection.data_too_new");
    assert_eq!(detail["file"], ConnectionFile::Peers.name());
    assert_eq!(detail["found"], 2);
    assert_eq!(detail["supported"], 1);
}

#[test]
fn a_too_new_room_file_is_refused_the_same_way() {
    let data_dir = configured("too-new-rooms");
    let path = data_dir.join("rooms.json");
    let original = r#"{"schema_version":4,"rooms":[]}"#;
    std::fs::write(&path, original).expect("write");

    let state = state_in(&unique_dir("too-new-rooms-ws"), &data_dir);

    assert!(state.rooms().is_none());
    assert_eq!(std::fs::read_to_string(&path).expect("read"), original);
    let detail = detail_of(&state, "host.connection.data_too_new");
    assert_eq!(detail["file"], ConnectionFile::Rooms.name());
    assert_eq!(detail["found"], 4);
    assert_eq!(detail["supported"], 1);
}

#[test]
fn a_too_new_node_key_is_refused_and_never_minted_over() {
    let data_dir = configured("too-new-key");
    let path = data_dir.join("node.key");
    let original = r#"{"schema_version":3,"kty":"OKP","crv":"Ed25519","x":"AA","d":"AA"}"#;
    std::fs::write(&path, original).expect("write");

    let state = state_in(&unique_dir("too-new-key-ws"), &data_dir);

    assert!(
        state.node_key().is_none(),
        "no key to hand out — and none was minted"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        original,
        "a newer key file is never written over"
    );
    let detail = detail_of(&state, "host.connection.data_too_new");
    assert_eq!(detail["file"], ConnectionFile::NodeKey.name());
    assert_eq!(detail["found"], 3);
}

#[test]
fn an_unusable_peer_table_is_reported_without_claiming_it_is_too_new() {
    let data_dir = configured("unusable");
    // A peer entry carrying a private key: `net` refuses it by name, and that refusal is not
    // "this file is from a newer build" — the two reports must not be conflated.
    let path = data_dir.join("peers.json");
    std::fs::write(
        &path,
        r#"{"schema_version":1,"peers":[{"node_id":"dev-b","addresses":[],"public_key":{"kty":"OKP","crv":"Ed25519","x":"AA","d":"AA"}}]}"#,
    )
    .expect("write");

    let state = state_in(&unique_dir("unusable-ws"), &data_dir);

    assert!(state.peers().is_none());
    let problem = state.connection_problem().expect("a reported problem");
    assert!(problem.contains("not usable"), "{problem}");
    assert!(
        !actions(&state)
            .iter()
            .any(|action| *action == "host.connection.data_too_new"),
        "no too-new row for a file that is not too new"
    );
}

#[test]
fn a_pointer_at_a_peer_this_node_does_not_know_is_refused() {
    let data_dir = configured_pointing("pointer-unknown", "dev-server");
    // A peer table with somebody else in it: the pointer names a node this host does not hold.
    let mut peers = net::PeersFile::empty();
    let other = net::NodeKey::generate().expect("key");
    peers.peers.push(net::PeerEntry::new(
        "dev-other",
        "127.0.0.1:1",
        other.public_jwk(),
    ));
    net::PeersFile::save_in(&data_dir, &peers).expect("peers.json");

    let state = state_in(&unique_dir("pointer-unknown-ws"), &data_dir);

    assert!(
        state.connection_client().is_none(),
        "no client for a peer this node does not know"
    );
    let problem = state.connection_problem().expect("a reported problem");
    assert!(problem.contains("dev-server"), "{problem}");
    assert!(problem.contains("peers.json"), "{problem}");
}

#[test]
fn a_pointer_at_a_known_peer_wires_a_client_that_registers_and_beats() {
    // The whole of V-2's client half, against a real server on loopback.
    let data_dir = configured_pointing("wired", "server");
    let workspace = unique_dir("wired-ws");

    // The node's key is minted **before** the state is built, so the server can be told its public half
    // the way §6.4 says a server is told: in the server's own `peers.json`.
    let node_key = net::NodeKey::generate().expect("key");
    net::NodeKey::save_new_in(&data_dir, &node_key).expect("node.key");

    let server_key = net::NodeKey::generate().expect("key");
    let mut server_peers = net::PeersFile::empty();
    server_peers.peers.push(net::PeerEntry::new(
        &agent::device(),
        "127.0.0.1:1",
        node_key.public_jwk(),
    ));
    let mut rooms = net::RoomsFile::empty();
    rooms.rooms.push(net::Room {
        name: "lab".to_string(),
        members: vec![agent::device()],
        rules: net::RoomRules::new(net::RateRule {
            messages: 10,
            window_seconds: 60,
        }),
    });

    let listener = net::Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let server = net::RelayServer::new(
        "server",
        server_key.clone(),
        server_peers,
        rooms.clone(),
        net::TransportConfig::default(),
    )
    .expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });

    // This node's own tables: the server as a peer, and the room the node is in.
    let mut local_peers = net::PeersFile::empty();
    local_peers.peers.push(net::PeerEntry::new(
        "server",
        &addr,
        server_key.public_jwk(),
    ));
    net::PeersFile::save_in(&data_dir, &local_peers).expect("peers.json");
    net::RoomsFile::save_in(&data_dir, &rooms).expect("rooms.json");

    let state = state_in(&workspace, &data_dir);
    assert!(
        state.connection_client().is_some(),
        "the pointer wired a client"
    );
    assert_eq!(state.connection_problem(), None);

    // The loop registers and beats (§6.6), and the row carries this node's claims.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while server.online().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the node never registered"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let row = server.online().into_iter().next().expect("a row");
    assert_eq!(row.node_id, agent::device());
    assert_eq!(
        row.rooms,
        vec!["lab".to_string()],
        "the room comes from this node's rooms.json"
    );
    assert!(
        row.addresses.is_empty(),
        "no peer port has landed yet, so none is claimed"
    );
    assert_eq!(row.state, net::Online::Online);

    // Restart the loop with a short interval so the beat can be watched, then stop it and watch the row
    // freeze: nothing else advances it.
    assert!(state.start_connection_heartbeat(Duration::from_millis(120)));
    let before = server.online()[0].last_heartbeat_ms;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while server.online()[0].last_heartbeat_ms == before {
        assert!(
            std::time::Instant::now() < deadline,
            "the short beat never landed"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    state.stop_connection_heartbeat();
    let stopped = server.online()[0].last_heartbeat_ms;
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        server.online()[0].last_heartbeat_ms,
        stopped,
        "the loop is stopped, so nothing beats"
    );
}
