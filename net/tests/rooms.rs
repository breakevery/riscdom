//! `rooms.json` and its three rules (v1.0 M4c; [connection.md §5](`../docs/connection.md`)).

use net::{
    consider_announcement, Mention, NodeKey, PeerEntry, PeersFile, RateCounters, RateRule, Room,
    RoomFilter, RoomRules, RoomsFile, VersionedLoad,
};
use std::path::PathBuf;

const NOW: i64 = 1_700_000_000_000;

fn scratch_dir(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "riscdom-net-rooms-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn rules(messages: u32, window_seconds: u32, mention: Mention) -> RoomRules {
    RoomRules {
        rate: RateRule {
            messages,
            window_seconds,
        },
        mention,
        require_signature: true,
    }
}

fn room(name: &str, members: &[&str], mention: Mention) -> Room {
    Room {
        name: name.to_string(),
        members: members.iter().map(|m| m.to_string()).collect(),
        rules: rules(10, 60, mention),
    }
}

#[test]
fn a_room_file_round_trips_and_answers_missing_and_newer_honestly() {
    let dir = scratch_dir("file");
    let mut file = RoomsFile::empty();
    file.rooms
        .push(room("lab", &["dev-a", "dev-b"], Mention::Members));
    RoomsFile::save_in(&dir, &file).expect("save");

    match RoomsFile::load_in(&dir).expect("load") {
        VersionedLoad::Current(read) => assert_eq!(read, file),
        other => panic!("expected the file back, got {other:?}"),
    }

    // The written bytes put `schema_version` first, like every other format here.
    let raw = std::fs::read_to_string(dir.join("rooms.json")).expect("read");
    assert!(raw.starts_with("{\n  \"schema_version\": 1,"), "{raw}");
    assert!(raw.contains("\"mention\": \"members\""), "{raw}");

    let empty = scratch_dir("missing");
    assert_eq!(
        RoomsFile::load_in(&empty).expect("load"),
        VersionedLoad::Missing
    );
    assert!(!empty.join("rooms.json").exists());

    let newer = scratch_dir("too-new");
    std::fs::write(
        newer.join("rooms.json"),
        r#"{"schema_version": 2, "rooms": []}"#,
    )
    .expect("write");
    assert_eq!(
        RoomsFile::load_in(&newer).expect("load"),
        VersionedLoad::TooNew { found: 2 }
    );
}

#[test]
fn require_signature_false_is_refused_because_it_would_lower_the_floor() {
    let dir = scratch_dir("unsigned");
    let mut file = RoomsFile::empty();
    let mut lab = room("lab", &["dev-a"], Mention::Nobody);
    lab.rules.require_signature = false;
    file.rooms.push(lab);
    // Written by hand: the loader is what must refuse it, not the writer.
    std::fs::write(
        dir.join("rooms.json"),
        serde_json::to_string_pretty(&file).expect("json"),
    )
    .expect("write");

    let error = RoomsFile::load_in(&dir).expect_err("unsigned room");
    assert!(
        error.to_string().contains("require_signature: false"),
        "the refusal names the field: {error}"
    );
    assert!(!error.to_string().contains("§5.2 must be relaxed"));
}

#[test]
fn a_nameless_room_a_duplicate_a_bad_rate_and_a_bad_mention_are_all_refused() {
    let dir = scratch_dir("malformed");
    let write = |raw: &str| {
        std::fs::write(dir.join("rooms.json"), raw).expect("write");
        RoomsFile::load_in(&dir).expect_err("refused")
    };

    // No name.
    assert!(
        write(r#"{"schema_version":1,"rooms":[{"name":"","members":["a"],"rules":{"rate":{"messages":1,"window_seconds":1},"mention":"members","require_signature":true}}]}"#)
            .to_string()
            .contains("empty name")
    );
    // Two rooms with one name.
    assert!(
        write(r#"{"schema_version":1,"rooms":[{"name":"lab","members":["a"],"rules":{"rate":{"messages":1,"window_seconds":1},"mention":"members","require_signature":true}},{"name":"lab","members":["b"],"rules":{"rate":{"messages":1,"window_seconds":1},"mention":"members","require_signature":true}}]}"#)
            .to_string()
            .contains("two rooms")
    );
    // A rate of zero messages, and a window of zero seconds.
    assert!(
        write(r#"{"schema_version":1,"rooms":[{"name":"lab","members":["a"],"rules":{"rate":{"messages":0,"window_seconds":60},"mention":"members","require_signature":true}}]}"#)
            .to_string()
            .contains("rate of 0")
    );
    assert!(
        write(r#"{"schema_version":1,"rooms":[{"name":"lab","members":["a"],"rules":{"rate":{"messages":1,"window_seconds":0},"mention":"members","require_signature":true}}]}"#)
            .to_string()
            .contains("window of 0")
    );
    // A mention that is neither `members` nor `nobody`.
    assert!(
        write(r#"{"schema_version":1,"rooms":[{"name":"lab","members":["a"],"rules":{"rate":{"messages":1,"window_seconds":1},"mention":"everyone","require_signature":true}}]}"#)
            .to_string()
            .contains("malformed")
    );
    // An empty member.
    assert!(
        write(r#"{"schema_version":1,"rooms":[{"name":"lab","members":[""],"rules":{"rate":{"messages":1,"window_seconds":1},"mention":"members","require_signature":true}}]}"#)
            .to_string()
            .contains("empty member")
    );
}

#[test]
fn the_filter_reads_membership_out_of_the_file() {
    let mut file = RoomsFile::empty();
    file.rooms.push(room("lab", &["dev-a"], Mention::Members));
    file.rooms.push(room("other", &["dev-b"], Mention::Members));
    // A room named but not listing this node: not one it is configured for.
    file.rooms.push(room("quiet", &["dev-b"], Mention::Nobody));

    let mine = RoomFilter::from_rooms(&file, "dev-a");
    assert_eq!(mine.len(), 1);
    assert!(mine.contains("lab"));
    assert!(!mine.contains("other"));
    assert!(!mine.contains("quiet"), "named is not the same as member");

    // Default deny: a node in nothing adopts nothing.
    let nothing = RoomFilter::from_rooms(&RoomsFile::empty(), "dev-a");
    assert!(nothing.is_empty());
    assert!(!nothing.admits(&["lab".to_string()]));

    // And the filter decides what a beacon may do.
    let key = NodeKey::generate().expect("key");
    let local = PeersFile::empty();
    let body = net::announcement_body(
        &PeerEntry::new("dev-x", "127.0.0.1:1", key.public_jwk()),
        &["lab".to_string()],
    );
    assert!(consider_announcement(&body, &local, &nothing)
        .expect("decide")
        .ignored());
    assert!(!consider_announcement(&body, &local, &mine)
        .expect("decide")
        .ignored());
}

#[test]
fn the_rate_rule_budgets_each_member_and_resets_with_the_window() {
    let mut counters = RateCounters::new();
    let rule = RateRule {
        messages: 3,
        window_seconds: 60,
    };
    for n in 0..3 {
        assert!(
            counters.admit("lab", "dev-a", &rule, NOW + n).is_ok(),
            "{n}"
        );
    }
    let refused = counters
        .admit("lab", "dev-a", &rule, NOW + 3)
        .expect_err("over budget");
    assert_eq!(refused.category(), net::Category::Refused);
    assert_eq!(refused.room, "lab");
    assert_eq!(refused.member, "dev-a");
    assert!(refused.to_string().contains("3 messages"));

    // Another member's budget is its own…
    assert!(counters.admit("lab", "dev-b", &rule, NOW + 3).is_ok());
    // …and another room's is too.
    assert!(counters.admit("other", "dev-a", &rule, NOW + 3).is_ok());
    // The window moves on: the count starts again.
    assert!(counters.admit("lab", "dev-a", &rule, NOW + 60_000).is_ok());
    assert_eq!(counters.used("lab", "dev-a"), 1);
}

#[test]
fn the_mention_rule_is_membership_then_permission() {
    let closed = room("lab", &["dev-a"], Mention::Nobody);
    assert!(!closed.allows_mention_from("dev-a"), "the default forbids");
    assert!(!closed.allows_mention_from("dev-b"), "and non-members too");

    let open = room("lab", &["dev-a"], Mention::Members);
    assert!(open.allows_mention());
    assert!(open.allows_mention_from("dev-a"));
    assert!(!open.allows_mention_from("dev-b"), "membership comes first");
    assert!(open.has_member("dev-a"));
}

#[test]
fn a_room_that_does_not_state_a_mention_gets_nobody() {
    let dir = scratch_dir("defaults");
    std::fs::write(
        dir.join("rooms.json"),
        r#"{"schema_version":1,"rooms":[{"name":"lab","members":["dev-a"],"rules":{"rate":{"messages":5,"window_seconds":30},"require_signature":true}}]}"#,
    )
    .expect("write");
    let file = match RoomsFile::load_in(&dir).expect("load") {
        VersionedLoad::Current(file) => file,
        other => panic!("expected a current file, got {other:?}"),
    };
    let lab = file.room("lab").expect("lab");
    assert_eq!(lab.rules.mention, Mention::Nobody);
    assert!(!lab.allows_mention_from("dev-a"));
    assert!(lab.rules.requires_signature());
    assert!(file.is_configured_for("dev-a", "lab"));
    assert!(!file.is_configured_for("dev-b", "lab"));
    assert!(!file.is_configured_for("dev-a", "no-such-room"));
}
