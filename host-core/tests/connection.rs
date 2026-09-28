//! The connection layer's files, as the host loads them (v1.0 batch W).
//!
//! [connection.md §2](../../docs/connection.md) gives a node its key on the first start with
//! networking configured; §4 and §5 put the peer table and the rooms beside it. These check the
//! host's half of that: **when** the three are read (the network settings are the switch),
//! what a refusal does (nothing is written over a newer file, and the refusal is visible), and
//! what the host says about it (two event names, and a problem a caller can read).

use host_core::settings::{LocalSettings, NetworkSettings, ServerRoleSettings, SETTINGS_VERSION};
use host_core::state::AppState;
use host_core::{ConnectionFile, EventFilter};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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

/// Sign `body` from `from` at `ts` and hand it to the server, without reading the answer.
///
/// A registration is answered down the sender's session, which the server here does not have; the
/// row is created before the acknowledgement is written, so the reply is deliberately ignored.
fn tell(
    server: &net::RelayServer,
    key: &net::NodeKey,
    from: &str,
    ts: i64,
    body: serde_json::Value,
) {
    let message = net::SignedMessage::sign(key, from, "server", ts, body).expect("sign");
    let frame = message.to_line().expect("line");
    let routed = server.route(&frame, ts).expect("routed");
    let action = routed.local().expect("a local frame").clone();
    let _ = server.answer_local(routed.from(), &action, ts);
}

/// The same, for a frame whose answer is an `Ok` the test reads.
fn route_local(
    server: &net::RelayServer,
    key: &net::NodeKey,
    from: &str,
    ts: i64,
    body: serde_json::Value,
) -> net::LocalReply {
    let message = net::SignedMessage::sign(key, from, "server", ts, body).expect("sign");
    let frame = message.to_line().expect("line");
    let routed = server.route(&frame, ts).expect("routed");
    let action = routed.local().expect("a local frame").clone();
    server
        .answer_local(routed.from(), &action, ts)
        .expect("answered")
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

#[test]
fn a_judgement_writes_its_two_events_through_the_servers_sink() {
    // The judging server is the one that records a judgement, and a deployment that runs a server
    // beside a chain installs this state's sink on it (v1.0 V-3a). The pointer names a peer this
    // node does not hold, so no client and no threads are wired: the test drives the server with
    // an explicit `now` instead.
    let data_dir = configured_pointing("judge-events", "ghost");
    let workspace = unique_dir("judge-events-ws");
    let b = net::NodeKey::generate().expect("key");
    let mut peers = net::PeersFile::empty();
    peers
        .peers
        .push(net::PeerEntry::new("dev-b", "127.0.0.1:2", b.public_jwk()));
    net::PeersFile::save_in(&data_dir, &peers).expect("peers.json");

    let state = Arc::new(state_in(&workspace, &data_dir));
    assert!(
        state.connection_client().is_none(),
        "a pointer at a peer we do not hold wires no client"
    );
    let node_key = state.node_key().expect("a key");
    let device = agent::device();

    // The server knows this node and dev-b, and its sink is this state's.
    let server_key = net::NodeKey::generate().expect("key");
    let mut server_peers = net::PeersFile::empty();
    server_peers.peers.push(net::PeerEntry::new(
        &device,
        "127.0.0.1:1",
        node_key.public_jwk(),
    ));
    server_peers
        .peers
        .push(net::PeerEntry::new("dev-b", "127.0.0.1:2", b.public_jwk()));
    let server = net::RelayServer::new(
        "server",
        server_key,
        server_peers,
        net::RoomsFile::empty(),
        net::TransportConfig::default(),
    )
    .expect("server");
    server.set_transition_sink(state.connection_judgement_sink());

    const T0: i64 = 1_700_000_000_000;
    tell(
        &server,
        &node_key,
        &device,
        T0,
        net::register_body(&[], &[], &[]),
    );
    tell(&server, &b, "dev-b", T0, net::register_body(&[], &[], &[]));

    // The witness reports the subject unreachable; the server judges, and the row lands in *this*
    // state's chain.
    let reply = route_local(
        &server,
        &node_key,
        &device,
        T0 + 1_000,
        net::unreachable_body("dev-b"),
    );
    assert!(
        matches!(reply, net::LocalReply::UnreachableReported { .. }),
        "{reply:?}"
    );
    let offline = detail_of(&state, "host.connection.peer_offline");
    assert_eq!(offline["peer"], "dev-b");
    assert_eq!(offline["witnesses"], serde_json::json!([device]));
    assert_eq!(offline["reports"], 1);

    // Recovery is being heard from, and the row names what was heard.
    let _ = route_local(&server, &b, "dev-b", T0 + 2_000, net::heartbeat_body());
    let recovered = detail_of(&state, "host.connection.peer_recovered");
    assert_eq!(recovered["peer"], "dev-b");
    assert_eq!(recovered["method"], "heartbeat");
}

#[test]
fn the_probe_thread_starts_only_with_a_workgroup_and_stops() {
    // No client (the pointer names a peer this node does not hold): nothing to probe with.
    let lonely = configured_pointing("probe-no-client", "ghost");
    let lonely_state = state_in(&unique_dir("probe-no-client-ws"), &lonely);
    assert!(
        !lonely_state.start_connection_probe(Duration::from_millis(50)),
        "a node with no client has no workgroup to probe"
    );

    // A wired node whose peer table holds a peer other than the server: the prober starts, and
    // stops. `stop` is called twice on purpose — stopping a stopped prober is a no-op.
    let data_dir = configured_pointing("probe-wired", "server");
    let workspace = unique_dir("probe-wired-ws");
    let node_key = net::NodeKey::generate().expect("key");
    net::NodeKey::save_new_in(&data_dir, &node_key).expect("node.key");
    let server_key = net::NodeKey::generate().expect("key");
    let b = net::NodeKey::generate().expect("key");

    let mut server_peers = net::PeersFile::empty();
    server_peers.peers.push(net::PeerEntry::new(
        &agent::device(),
        "127.0.0.1:1",
        node_key.public_jwk(),
    ));
    server_peers
        .peers
        .push(net::PeerEntry::new("dev-b", "127.0.0.1:2", b.public_jwk()));
    let listener = net::Listener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr").to_string();
    let server = net::RelayServer::new(
        "server",
        server_key.clone(),
        server_peers,
        net::RoomsFile::empty(),
        net::TransportConfig::default(),
    )
    .expect("server");
    let serving = server.clone();
    std::thread::spawn(move || {
        let _ = serving.serve(listener);
    });

    let mut local_peers = net::PeersFile::empty();
    local_peers.peers.push(net::PeerEntry::new(
        "server",
        &addr,
        server_key.public_jwk(),
    ));
    local_peers
        .peers
        .push(net::PeerEntry::new("dev-b", "127.0.0.1:2", b.public_jwk()));
    net::PeersFile::save_in(&data_dir, &local_peers).expect("peers.json");

    let state = state_in(&workspace, &data_dir);
    assert!(
        state.connection_client().is_some(),
        "the pointer wired a client"
    );
    assert!(
        state.start_connection_probe(Duration::from_millis(50)),
        "a workgroup starts the prober"
    );
    state.stop_connection_probe();
    assert!(
        state.start_connection_probe(Duration::from_millis(50)),
        "the prober can be started again"
    );
    state.stop_connection_probe();
}

/// A data directory whose settings give the node a **server role** (v1.0 AC-4).
fn configured_server_role(tag: &str, bind: &str) -> PathBuf {
    let dir = unique_dir(tag);
    let settings = LocalSettings {
        version: SETTINGS_VERSION,
        network: Some(NetworkSettings {
            lan_enabled: true,
            server_role: Some(ServerRoleSettings {
                bind: bind.to_string(),
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    settings
        .save(&dir.join("settings.json"))
        .expect("settings.json");
    dir
}

#[test]
fn a_configured_server_role_binds_and_registers_a_node() {
    // v1.0 AC-4: a node whose settings give it a **server role** serves its workgroup — the same
    // `RelayServer` the standalone `riscdom-relay` deployment runs, started from this node's own
    // files (`node.key`, `peers.json`, `rooms.json`). The bind is `127.0.0.1:0`, so the test reads
    // the address the operating system chose rather than fixing one.
    let data_dir = configured_server_role("server-role", "127.0.0.1:0");
    let server_key = net::NodeKey::generate().expect("key");
    net::NodeKey::save_new_in(&data_dir, &server_key).expect("node.key");

    // The node the server will authenticate, in the server's own `peers.json` (§6.3), and the
    // room it claims, in `rooms.json`.
    let client_key = net::NodeKey::generate().expect("key");
    let mut peers = net::PeersFile::empty();
    peers.peers.push(net::PeerEntry::new(
        "dev-b",
        "127.0.0.1:2",
        client_key.public_jwk(),
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

    let state = state_in(&unique_dir("server-role-ws"), &data_dir);
    let addr = state
        .server_role_addr()
        .expect("the configured server role is serving");
    let server = state.server_role().expect("the handle is kept");
    assert_eq!(state.connection_problem(), None);

    // A node dials it and registers (§6.6); the row lands in *this node's* server.
    let entry = net::PeerEntry::new(&agent::device(), &addr, server_key.public_jwk());
    let client =
        net::RelayClient::new("dev-b", client_key, &entry, net::TransportConfig::default())
            .expect("client");
    client
        .register(&net::Registration::in_rooms(["lab".to_string()]))
        .expect("register");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while server.online().is_empty() {
        assert!(
            std::time::Instant::now() < deadline,
            "the node never registered"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let row = server.online().into_iter().next().expect("a row");
    assert_eq!(row.node_id, "dev-b");
    assert_eq!(row.rooms, vec!["lab".to_string()]);
    assert_eq!(row.state, net::Online::Online);

    // A beat is accepted too — §6.6 answers it with nothing, so the row is what says it landed.
    client.heartbeat().expect("heartbeat");
}

#[test]
fn a_node_without_a_server_role_serves_nobody() {
    // The other half of the switch: nothing is configured, so nothing is bound and no address
    // exists — the project never starts a server a deployer did not ask for.
    let state = state_in(&unique_dir("no-role-ws"), &configured("no-role"));
    assert_eq!(state.server_role_addr(), None);
    assert!(state.server_role().is_none());
}

#[test]
fn a_server_role_that_cannot_bind_is_reported_and_the_node_runs() {
    // A bind that cannot be taken is a reported problem, not a silent thread: the state is still
    // built, and `connection_problem` says what happened.
    let data_dir = configured_server_role("server-role-bad-bind", "256.256.256.256:1");
    let key = net::NodeKey::generate().expect("key");
    net::NodeKey::save_new_in(&data_dir, &key).expect("node.key");
    let state = state_in(&unique_dir("server-role-bad-bind-ws"), &data_dir);
    assert_eq!(state.server_role_addr(), None);
    let problem = state.connection_problem().expect("a reported problem");
    assert!(problem.contains("server_role"), "{problem}");
}
