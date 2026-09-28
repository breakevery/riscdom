//! Discovery: the peer table, the handed-down node table, the UDP beacon and the room
//! filter (v1.0 M4b; [connection.md §4](`../docs/connection.md`)).

use net::{
    consider_announcement, merge_table, receive_datagram, send_datagram, sign_announcement,
    verify_at, Adoption, NodeKey, NodeTable, PeerEntry, PeerKeys, PeersFile, ReplayGuard,
    RoomFilter, VersionedLoad,
};
use std::net::UdpSocket;
use std::path::PathBuf;

const NOW: i64 = 1_700_000_000_000;

fn scratch_dir(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "riscdom-net-discovery-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn entry(node_id: &str, key: &NodeKey, address: &str) -> PeerEntry {
    PeerEntry::new(node_id, address, key.public_jwk())
}

#[test]
fn a_peer_file_round_trips_and_a_missing_or_newer_one_is_answered_honestly() {
    let dir = scratch_dir("file");
    let key = NodeKey::generate().expect("key");
    let mut file = PeersFile::empty();
    file.peers.push(entry("dev-a", &key, "127.0.0.1:47821"));
    PeersFile::save_in(&dir, &file).expect("save");

    match PeersFile::load_in(&dir).expect("load") {
        VersionedLoad::Current(read) => assert_eq!(read, file),
        other => panic!("expected the file back, got {other:?}"),
    }

    // The written bytes put `schema_version` first, like every other format here.
    let raw = std::fs::read_to_string(dir.join("peers.json")).expect("read");
    assert!(raw.starts_with("{\n  \"schema_version\": 1,"), "{raw}");

    // Missing: nothing there, and nothing created.
    let empty = scratch_dir("missing");
    assert_eq!(
        PeersFile::load_in(&empty).expect("load"),
        VersionedLoad::Missing
    );
    assert!(!empty.join("peers.json").exists());

    // TooNew: refused, not half-read.
    let newer = scratch_dir("too-new");
    std::fs::write(
        newer.join("peers.json"),
        r#"{"schema_version": 2, "peers": []}"#,
    )
    .expect("write");
    assert_eq!(
        PeersFile::load_in(&newer).expect("load"),
        VersionedLoad::TooNew { found: 2 }
    );
}

#[test]
fn a_peer_file_that_carries_a_private_key_is_refused() {
    let dir = scratch_dir("secret");
    let key = NodeKey::generate().expect("key");
    let mut public = key.public_jwk();
    public["d"] = serde_json::json!(key.d);
    let file = PeersFile {
        schema_version: PeersFile::SCHEMA_VERSION,
        peers: vec![PeerEntry::new("dev-a", "127.0.0.1:1", public)],
    };
    PeersFile::save_in(&dir, &file).expect("save");
    let error = PeersFile::load_in(&dir).expect_err("private key");
    assert!(
        error.to_string().contains("private key"),
        "the refusal names it: {error}"
    );
}

#[test]
fn the_local_file_wins_and_a_disagreement_is_reported() {
    let local_key = NodeKey::generate().expect("key");
    let other_key = NodeKey::generate().expect("key");
    let newcomer = NodeKey::generate().expect("key");

    let mut local = PeersFile::empty();
    local
        .peers
        .push(entry("dev-a", &local_key, "127.0.0.1:1111"));

    // The table says something different about dev-a, and something new about dev-c.
    let table = NodeTable::new(
        7,
        vec![
            entry("dev-a", &other_key, "127.0.0.1:2222"),
            entry("dev-c", &newcomer, "127.0.0.1:3333"),
        ],
    );

    let (merged, report) = merge_table(&local, &table);
    assert_eq!(report.added, 1, "dev-c is new");
    assert_eq!(report.unchanged, 0);
    assert!(report.has_conflicts(), "dev-a disagrees");
    assert_eq!(report.conflicts.len(), 1);
    assert_eq!(report.conflicts[0].node_id, "dev-a");
    assert!(
        report.conflicts[0].local.contains("1111"),
        "the report names both sides: {:?}",
        report.conflicts[0]
    );
    assert!(report.conflicts[0].from_table.contains("2222"));

    // The local entry is the one that survived.
    assert_eq!(
        merged.entry("dev-a").expect("dev-a").addresses,
        vec!["127.0.0.1:1111".to_string()],
        "local wins"
    );
    assert!(merged.entry("dev-c").is_some(), "the new node was added");
}

#[test]
fn a_table_that_agrees_adds_nothing_and_reports_nothing() {
    let key = NodeKey::generate().expect("key");
    let mut local = PeersFile::empty();
    local.peers.push(entry("dev-a", &key, "127.0.0.1:1111"));
    let table = NodeTable::new(3, vec![entry("dev-a", &key, "127.0.0.1:1111")]);

    let (merged, report) = merge_table(&local, &table);
    assert_eq!(report.unchanged, 1);
    assert_eq!(report.added, 0);
    assert!(!report.has_conflicts());
    assert_eq!(merged.peers.len(), 1);
}

#[test]
fn the_hand_down_travels_as_a_signed_frame() {
    let server = NodeKey::generate().expect("server key");
    let peer = NodeKey::generate().expect("peer key");
    let table = NodeTable::new(11, vec![entry("dev-a", &peer, "127.0.0.1:1111")]);
    assert!(table.is_newer_than(10));
    assert!(!table.is_newer_than(11), "the same generation is not newer");

    // The server signs the body as an ordinary frame — it is a peer whose key the node
    // knows.
    let signed = table
        .sign(&server, "server", "dev-b", NOW)
        .expect("signed table");
    let mut peers = PeerKeys::new();
    peers.insert("server", [server.verifying_key().expect("public")]);
    let mut guard = ReplayGuard::new();
    let verified = verify_at(&signed, "dev-b", &peers, &mut guard, NOW).expect("verified");

    let received = NodeTable::from_verified(&verified).expect("table");
    assert_eq!(received, table);

    // And a body that is not a table is refused rather than half-read.
    assert!(NodeTable::from_body(&serde_json::json!({ "peers": [] })).is_err());
}

#[test]
fn an_announcement_travels_as_one_signed_datagram() {
    let sender = NodeKey::generate().expect("sender");
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
    receiver_socket
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("timeout");
    let receiver_addr = receiver_socket.local_addr().expect("addr");
    let sender_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");

    let announced = entry("dev-a", &sender, &receiver_addr.to_string());
    let beacon = sign_announcement(&sender, "dev-a", NOW, &announced, &["lab".to_string()])
        .expect("announce");
    let sent = send_datagram(&sender_socket, receiver_addr, &beacon).expect("send");
    assert!(sent > 0);

    let mut buffer = vec![0u8; net::MAX_DATAGRAM_BYTES];
    let (received, from) = receive_datagram(&receiver_socket, &mut buffer).expect("receive");
    assert_eq!(
        from.ip(),
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
    );
    assert_eq!(received, beacon, "one datagram, one frame");

    // The beacon verifies like any other frame: `to` is the sender, because a beacon is
    // addressed to nobody in particular.
    let mut peers = PeerKeys::new();
    peers.insert("dev-a", [sender.verifying_key().expect("public")]);
    let mut guard = ReplayGuard::new();
    let verified = verify_at(&received, "dev-a", &peers, &mut guard, NOW).expect("verified");
    assert_eq!(verified.from, "dev-a");
}

#[test]
fn the_room_filter_decides_who_may_be_adopted() {
    let known = NodeKey::generate().expect("known");
    let stranger = NodeKey::generate().expect("stranger");
    let mut local = PeersFile::empty();
    local
        .peers
        .push(entry("dev-known", &known, "127.0.0.1:1111"));

    let body = |who: &str, key: &NodeKey, rooms: &[&str]| {
        net::announcement_body(
            &entry(who, key, "127.0.0.1:2222"),
            &rooms.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
        )
    };

    // No rooms configured: nothing is adopted, whatever is announced.
    let none = RoomFilter::none();
    let adoption =
        consider_announcement(&body("dev-known", &known, &["lab"]), &local, &none).expect("decide");
    assert!(adoption.ignored(), "default deny: {adoption:?}");

    // In the lab: a known node has its address refreshed…
    let lab = RoomFilter::of(["lab"]);
    let adoption =
        consider_announcement(&body("dev-known", &known, &["lab"]), &local, &lab).expect("decide");
    match &adoption {
        Adoption::RefreshedAddresses { node_id, addresses } => {
            assert_eq!(node_id, "dev-known");
            assert_eq!(addresses, &vec!["127.0.0.1:2222".to_string()]);
        }
        other => panic!("expected a refresh, got {other:?}"),
    }
    assert!(!adoption.introduced_a_key());

    // …and an unknown node is *reported*, never adopted.
    let adoption = consider_announcement(&body("dev-stranger", &stranger, &["lab"]), &local, &lab)
        .expect("decide");
    match &adoption {
        Adoption::ReportedUnknown {
            node_id,
            addresses,
            rooms,
        } => {
            assert_eq!(node_id, "dev-stranger");
            assert_eq!(addresses, &vec!["127.0.0.1:2222".to_string()]);
            assert_eq!(rooms, &vec!["lab".to_string()]);
        }
        other => panic!("expected a report, got {other:?}"),
    }
    assert!(
        !adoption.introduced_a_key(),
        "discovery is not a trust path"
    );
    assert!(adoption.refreshed().is_none());

    // Somebody else's room: ignored even though the node is known.
    let adoption = consider_announcement(&body("dev-known", &known, &["other"]), &local, &lab)
        .expect("decide");
    assert!(adoption.ignored(), "{adoption:?}");
}

#[test]
fn an_announcement_that_carries_a_private_key_is_refused() {
    let key = NodeKey::generate().expect("key");
    let mut public = key.public_jwk();
    public["d"] = serde_json::json!(key.d);
    let body = net::announcement_body(
        &PeerEntry::new("dev-a", "127.0.0.1:1", public),
        &["lab".to_string()],
    );
    let local = PeersFile::empty();
    let lab = RoomFilter::of(["lab"]);
    let error = consider_announcement(&body, &local, &lab).expect_err("private key");
    assert!(
        error.to_string().contains("private key"),
        "the refusal names it: {error}"
    );
}

#[test]
fn the_broadcast_port_is_a_protocol_constant() {
    // §4.3: a broadcast must reach a node that knows nothing yet, so its port cannot be
    // discovered and must not be a setting. The value is this build's; the *rule* is what
    // the constant says.
    assert_eq!(net::BROADCAST_PORT, 47821);
}
