//! Rooms, proved offline: the file, the three rules, and the filter that reads membership
//! (v1.0 M4c).
//!
//! ```text
//! cargo run -p net --example rooms -- --self-test
//! ```
//!
//! A scratch directory holds the `rooms.json`; everything else is in memory.
//! `scripts/gate.sh` runs this beside the other example proofs.

use net::{
    consider_announcement, Mention, NodeKey, PeerEntry, PeersFile, RateCounters, RateRule, Room,
    RoomFilter, RoomRules, RoomsFile, VersionedLoad,
};
use std::path::PathBuf;

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
        "riscdom-net-rooms-self-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn self_test() {
    // 1. The file: one version, rooms with members and rules, `schema_version` first.
    let dir = scratch_dir("file");
    let mut file = RoomsFile::empty();
    file.rooms.push(Room {
        name: "lab".into(),
        members: vec!["dev-a".into(), "dev-b".into()],
        rules: RoomRules {
            rate: RateRule {
                messages: 2,
                window_seconds: 60,
            },
            mention: Mention::Members,
            require_signature: true,
        },
    });
    file.rooms.push(Room {
        name: "quiet".into(),
        members: vec!["dev-b".into()],
        // `RoomRules::new` states no `mention`: the default is nobody.
        rules: RoomRules::new(RateRule {
            messages: 5,
            window_seconds: 30,
        }),
    });
    RoomsFile::save_in(&dir, &file).expect("save");
    let raw = std::fs::read_to_string(dir.join("rooms.json")).expect("read");
    let read = match RoomsFile::load_in(&dir).expect("load") {
        VersionedLoad::Current(read) => read,
        other => panic!("expected a current file, got {other:?}"),
    };
    check(
        "rooms.json reads back, with schema_version first",
        raw.starts_with("{\n  \"schema_version\": 1,") && read == file,
        raw.lines().next().unwrap_or_default().to_string(),
    );

    // 2. `require_signature: false` is refused: §3 floors it.
    let unsigned = scratch_dir("unsigned");
    let raw_unsigned = raw.replace(
        "\"require_signature\": true",
        "\"require_signature\": false",
    );
    std::fs::write(unsigned.join("rooms.json"), &raw_unsigned).expect("write");
    let error = RoomsFile::load_in(&unsigned).expect_err("unsigned");
    check(
        "a room may not drop the signature floor",
        error.to_string().contains("require_signature: false"),
        error.to_string(),
    );

    // 3. The filter reads membership out of the file.
    let mine = RoomFilter::from_rooms(&read, "dev-a");
    let theirs = RoomFilter::from_rooms(&read, "dev-c");
    let empty = RoomFilter::from_rooms(&RoomsFile::empty(), "dev-a");
    check(
        "the filter is the rooms this node is a member of",
        mine.contains("lab")
            && !mine.contains("quiet")
            && theirs.is_empty()
            && !empty.admits(&["lab".to_string()]),
        format!("dev-a is in {} room(s)", mine.len()),
    );

    // 4. The rate rule budgets each member, and over budget is `refused`.
    let lab = read.room("lab").expect("lab");
    let mut counters = RateCounters::new();
    let first = counters.admit("lab", "dev-a", &lab.rules.rate, NOW);
    let second = counters.admit("lab", "dev-a", &lab.rules.rate, NOW + 1);
    let third = counters.admit("lab", "dev-a", &lab.rules.rate, NOW + 2);
    let over = third.expect_err("over budget");
    check(
        "two messages fit the budget, the third is refused",
        first.is_ok()
            && second.is_ok()
            && over.category() == net::Category::Refused
            && over.to_string().contains("2 messages"),
        over.to_string(),
    );
    // Another member has its own budget, and the window resets.
    let other_member = counters.admit("lab", "dev-b", &lab.rules.rate, NOW + 2);
    let next_window = counters.admit("lab", "dev-a", &lab.rules.rate, NOW + 60_000);
    check(
        "the budget is per member and resets with the window",
        other_member.is_ok() && next_window.is_ok() && counters.used("lab", "dev-a") == 1,
        format!("used {} in the new window", counters.used("lab", "dev-a")),
    );

    // 5. The mention rule: membership first, then the setting.
    let quiet = read.room("quiet").expect("quiet");
    check(
        "mention is membership first, then the room's setting",
        lab.allows_mention_from("dev-a")
            && !lab.allows_mention_from("dev-c")
            && !quiet.allows_mention_from("dev-b"),
        format!(
            "lab/members: {}; quiet/default: {}",
            lab.allows_mention(),
            quiet.allows_mention()
        ),
    );

    // 6. And the filter decides what a beacon may do — the M4b loop, now closed by the file.
    let key = NodeKey::generate().expect("key");
    let local = PeersFile::empty();
    let body = net::announcement_body(
        &PeerEntry::new("dev-x", "127.0.0.1:1", key.public_jwk()),
        &["lab".to_string()],
    );
    let adopted = consider_announcement(&body, &local, &mine).expect("decide");
    let denied = consider_announcement(&body, &local, &empty).expect("decide");
    check(
        "a beacon from a room this node is in is considered; with none, nothing is",
        !adopted.ignored() && denied.ignored(),
        format!("{adopted:?} / {denied:?}"),
    );

    println!("net rooms self-test: OK");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        self_test();
        return;
    }
    println!("usage: cargo run -p net --example rooms -- --self-test");
    println!("reads rooms.json, applies the three rules, and feeds the discovery filter.");
}
