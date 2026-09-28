//! Discovery, proved offline: the peer table, the handed-down table, the UDP beacon and
//! the room filter (v1.0 M4b).
//!
//! ```text
//! cargo run -p net --example discovery -- --self-test
//! ```
//!
//! Everything is in-process or on loopback: a scratch directory for `peers.json`, two UDP
//! sockets bound to `127.0.0.1:0`, and in-memory keys. `scripts/gate.sh` runs it beside the
//! other example proofs.

use net::{
    consider_announcement, merge_table, receive_datagram, send_datagram, sign_announcement,
    verify_at, Adoption, NodeKey, NodeTable, PeerEntry, PeerKeys, PeersFile, ReplayGuard,
    RoomFilter, VersionedLoad,
};
use std::net::UdpSocket;
use std::path::PathBuf;
use std::time::Duration;

const NOW: i64 = 1_700_000_000_000;

fn check(what: &str, ok: bool, detail: String) {
    println!("{}  {what}: {detail}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}

fn scratch_dir(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "riscdom-net-discovery-self-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn entry(node_id: &str, key: &NodeKey, address: &str) -> PeerEntry {
    PeerEntry::new(node_id, address, key.public_jwk())
}

fn self_test() {
    let local_key = NodeKey::generate().expect("key");
    let other_key = NodeKey::generate().expect("key");

    // 1. peers.json is a versioned JSON file, `schema_version` first.
    let dir = scratch_dir("file");
    let mut file = PeersFile::empty();
    file.peers
        .push(entry("dev-a", &local_key, "127.0.0.1:47821"));
    PeersFile::save_in(&dir, &file).expect("save");
    let raw = std::fs::read_to_string(dir.join("peers.json")).expect("read");
    let read = PeersFile::load_in(&dir).expect("load");
    check(
        "peers.json reads back, with schema_version first",
        raw.starts_with("{\n  \"schema_version\": 1,")
            && read == VersionedLoad::Current(file.clone()),
        raw.lines().next().unwrap_or_default().to_string(),
    );

    // 2. A handed-down table is a source: the local file wins, the conflict is reported.
    let table = NodeTable::new(
        4,
        vec![
            entry("dev-a", &other_key, "127.0.0.1:9999"),
            entry("dev-new", &other_key, "127.0.0.1:8888"),
        ],
    );
    let (merged, report) = merge_table(&file, &table);
    check(
        "a conflict is reported and the local entry wins",
        report.has_conflicts()
            && report.added == 1
            && merged.entry("dev-a").expect("dev-a").addresses
                == vec!["127.0.0.1:47821".to_string()],
        format!(
            "{} conflict(s), {} added, local kept {:?}",
            report.conflicts.len(),
            report.added,
            merged.entry("dev-a").expect("dev-a").addresses
        ),
    );

    // 3. The hand-down travels as an ordinary signed frame from a known peer.
    let server = NodeKey::generate().expect("server");
    let signed = NodeTable::new(4, vec![entry("dev-new", &other_key, "127.0.0.1:8888")])
        .sign(&server, "server", "dev-a", NOW)
        .expect("sign");
    let mut peers = PeerKeys::new();
    peers.insert("server", [server.verifying_key().expect("public")]);
    let mut guard = ReplayGuard::new();
    let verified = verify_at(&signed, "dev-a", &peers, &mut guard, NOW).expect("verify");
    let received = NodeTable::from_verified(&verified).expect("table");
    check(
        "the table arrives as a verified signed frame",
        received.generation() == 4 && received.entries().len() == 1,
        format!("generation {}", received.generation()),
    );

    // 4. The beacon: one datagram, one signed frame, verified by the receiver.
    let receiver_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");
    receiver_socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let receiver_addr = receiver_socket.local_addr().expect("addr");
    let sender_socket = UdpSocket::bind("127.0.0.1:0").expect("bind");

    let announced = entry("dev-a", &local_key, "127.0.0.1:47821");
    let beacon = sign_announcement(&local_key, "dev-a", NOW, &announced, &["lab".to_string()])
        .expect("announce");
    let bytes = send_datagram(&sender_socket, receiver_addr, &beacon).expect("send");
    let mut buffer = vec![0u8; net::MAX_DATAGRAM_BYTES];
    let (received, _from) = receive_datagram(&receiver_socket, &mut buffer).expect("receive");

    let mut beacon_peers = PeerKeys::new();
    beacon_peers.insert("dev-a", [local_key.verifying_key().expect("public")]);
    let mut beacon_guard = ReplayGuard::new();
    // A beacon is addressed to nobody in particular, so its `to` is the sender.
    let verified = verify_at(&received, "dev-a", &beacon_peers, &mut beacon_guard, NOW);
    check(
        "an announcement arrives as a verified signed datagram",
        verified.is_ok() && received == beacon,
        format!("{bytes} bytes"),
    );

    // 5. The room filter, with default deny.
    let body = |who: &str, key: &NodeKey, rooms: &[&str]| {
        net::announcement_body(
            &entry(who, key, "127.0.0.1:7777"),
            &rooms.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
        )
    };
    let none = RoomFilter::none();
    let ignored =
        consider_announcement(&body("dev-a", &local_key, &["lab"]), &file, &none).expect("decide");
    check(
        "with no rooms configured, nothing is adopted",
        ignored.ignored(),
        format!("{ignored:?}"),
    );

    let lab = RoomFilter::of(["lab"]);
    let refreshed =
        consider_announcement(&body("dev-a", &local_key, &["lab"]), &file, &lab).expect("decide");
    let reported = consider_announcement(&body("dev-stranger", &other_key, &["lab"]), &file, &lab)
        .expect("decide");
    check(
        "a known node has its address refreshed; an unknown one is only reported",
        matches!(refreshed, Adoption::RefreshedAddresses { .. })
            && matches!(reported, Adoption::ReportedUnknown { .. })
            && !reported.introduced_a_key(),
        format!("{refreshed:?} / {reported:?}"),
    );

    let elsewhere =
        consider_announcement(&body("dev-a", &local_key, &["other"]), &file, &lab).expect("decide");
    check(
        "somebody else's room is ignored",
        elsewhere.ignored(),
        format!("{elsewhere:?}"),
    );

    // 6. The broadcast port is a constant, not a setting.
    check(
        "the broadcast port is a protocol constant",
        net::BROADCAST_PORT == 47821,
        net::BROADCAST_PORT.to_string(),
    );

    println!("net discovery self-test: OK");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        self_test();
        return;
    }
    println!("usage: cargo run -p net --example discovery -- --self-test");
    println!("reads a peer table, merges a hand-down and moves a beacon over UDP.");
}
