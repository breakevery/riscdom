//! Signing, the six verification steps, and replay protection (v1.0 M4a;
//! [connection.md §3](`../docs/connection.md`) and §3.2).

use net::{
    body_hash, verify_at, Category, NodeKey, PeerKeys, ReplayGuard, SignedMessage, VerifyError,
    PROTOCOL_VERSION, REPLAY_WINDOW_AHEAD_MS, REPLAY_WINDOW_BACK_MS,
};
use serde_json::json;

/// A fixed "now" so a test can place a message anywhere in the window.
const NOW: i64 = 1_700_000_000_000;

fn peers_with(node_id: &str, key: &NodeKey) -> PeerKeys {
    let mut peers = PeerKeys::new();
    peers.insert(node_id, [key.verifying_key().expect("verifying key")]);
    peers
}

/// A message signed by `key`, from `who` to `us`, stamped `ts`.
fn message(key: &NodeKey, who: &str, us: &str, ts: i64, body: serde_json::Value) -> SignedMessage {
    SignedMessage::sign(key, who, us, ts, body).expect("sign")
}

#[test]
fn a_signed_message_verifies_and_answers_with_its_payload() {
    let key = NodeKey::generate().expect("key");
    let mut guard = ReplayGuard::new();
    let signed = message(&key, "dev-a", "dev-b", NOW, json!({ "hello": "there" }));

    let verified = verify_at(
        &signed,
        "dev-b",
        &peers_with("dev-a", &key),
        &mut guard,
        NOW,
    )
    .expect("verify");
    assert_eq!(verified.from, "dev-a");
    assert_eq!(verified.to, "dev-b");
    assert_eq!(verified.ts, NOW);
    assert_eq!(verified.body, json!({ "hello": "there" }));
    assert_eq!(guard.tracked_peers(), 1, "the sender's mark is recorded");
}

#[test]
fn step_one_an_unknown_sender_is_refused() {
    let key = NodeKey::generate().expect("key");
    let other = NodeKey::generate().expect("key");
    let mut guard = ReplayGuard::new();
    // `other` is not in the table: the message is well-formed and correctly signed,
    // and it is still refused.
    let signed = message(&other, "dev-stranger", "dev-b", NOW, json!({}));
    let refused = verify_at(
        &signed,
        "dev-b",
        &peers_with("dev-a", &key),
        &mut guard,
        NOW,
    )
    .expect_err("unknown");
    assert!(matches!(refused, VerifyError::UnknownSender(ref who) if who == "dev-stranger"));
    assert_eq!(refused.category(), Category::Refused);
}

#[test]
fn step_two_a_broken_signature_is_invalid() {
    let key = NodeKey::generate().expect("key");
    let peers = peers_with("dev-a", &key);
    let mut guard = ReplayGuard::new();

    // The payload is edited after signing, so the signature covers something else.
    let mut tampered = message(&key, "dev-a", "dev-b", NOW, json!({ "amount": 1 }));
    tampered.body = json!({ "amount": 2 });
    let broken = verify_at(&tampered, "dev-b", &peers, &mut guard, NOW).expect_err("tampered");
    assert!(matches!(broken, VerifyError::BadSignature(_)));
    assert_eq!(broken.category(), Category::Invalid);

    // A signature from somebody else's key fails the same way.
    let other = NodeKey::generate().expect("key");
    let mut swapped = message(&key, "dev-a", "dev-b", NOW, json!({}));
    swapped.sig = message(&other, "dev-a", "dev-b", NOW, json!({})).sig;
    let broken = verify_at(&swapped, "dev-b", &peers, &mut guard, NOW).expect_err("swapped");
    assert!(matches!(broken, VerifyError::BadSignature(_)));
}

#[test]
fn step_three_an_unsupported_version_is_invalid() {
    let key = NodeKey::generate().expect("key");
    let mut guard = ReplayGuard::new();
    // Signed **at** the newer version, so the signature checks out and step 3 is the
    // step that refuses: that is the only order in which "wrong version" is
    // distinguishable from "forged".
    let newer = SignedMessage::sign_with_version(
        PROTOCOL_VERSION + 1,
        &key.signing_key().expect("signing key"),
        "dev-a",
        "dev-b",
        NOW,
        json!({}),
    );
    let refused = verify_at(&newer, "dev-b", &peers_with("dev-a", &key), &mut guard, NOW)
        .expect_err("newer version");
    assert!(matches!(
        refused,
        VerifyError::UnsupportedVersion { found, supported }
            if found == PROTOCOL_VERSION + 1 && supported == PROTOCOL_VERSION
    ));
    assert_eq!(refused.category(), Category::Invalid);

    // Editing `v` on a signed message breaks the signature instead — step 2 — because
    // the version is inside the bytes that were signed.
    let mut edited = message(&key, "dev-a", "dev-b", NOW, json!({}));
    edited.v = PROTOCOL_VERSION + 1;
    assert!(matches!(
        verify_at(
            &edited,
            "dev-b",
            &peers_with("dev-a", &key),
            &mut guard,
            NOW
        )
        .expect_err("edited version"),
        VerifyError::BadSignature(_)
    ));
}

#[test]
fn step_four_a_message_addressed_elsewhere_is_invalid() {
    let key = NodeKey::generate().expect("key");
    let mut guard = ReplayGuard::new();
    let signed = message(&key, "dev-a", "dev-c", NOW, json!({}));
    let refused = verify_at(
        &signed,
        "dev-b",
        &peers_with("dev-a", &key),
        &mut guard,
        NOW,
    )
    .expect_err("addressed elsewhere");
    assert!(matches!(
        refused,
        VerifyError::NotAddressedToUs { ref to, ref us } if to == "dev-c" && us == "dev-b"
    ));
    assert_eq!(refused.category(), Category::Invalid);
}

#[test]
fn step_five_a_stale_or_future_timestamp_is_network() {
    let key = NodeKey::generate().expect("key");
    let peers = peers_with("dev-a", &key);
    let mut guard = ReplayGuard::new();

    let stale = message(
        &key,
        "dev-a",
        "dev-b",
        NOW - REPLAY_WINDOW_BACK_MS - 1,
        json!({}),
    );
    let refused = verify_at(&stale, "dev-b", &peers, &mut guard, NOW).expect_err("stale");
    assert!(matches!(refused, VerifyError::Stale { .. }));
    assert_eq!(refused.category(), Category::Network);

    let future = message(
        &key,
        "dev-a",
        "dev-b",
        NOW + REPLAY_WINDOW_AHEAD_MS + 1,
        json!({}),
    );
    let refused = verify_at(&future, "dev-b", &peers, &mut guard, NOW).expect_err("future");
    assert!(matches!(refused, VerifyError::Future { .. }));
    assert_eq!(refused.category(), Category::Network);

    // Neither refusal advanced the mark: a legitimate message still gets in at NOW.
    assert!(guard.high_water("dev-a").is_none());
    assert!(verify_at(
        &message(&key, "dev-a", "dev-b", NOW, json!({})),
        "dev-b",
        &peers,
        &mut guard,
        NOW
    )
    .is_ok());
}

#[test]
fn step_six_a_replay_is_refused() {
    let key = NodeKey::generate().expect("key");
    let peers = peers_with("dev-a", &key);
    let mut guard = ReplayGuard::new();
    let signed = message(&key, "dev-a", "dev-b", NOW, json!({ "once": true }));

    assert!(verify_at(&signed, "dev-b", &peers, &mut guard, NOW).is_ok());
    let refused =
        verify_at(&signed, "dev-b", &peers, &mut guard, NOW).expect_err("the second time");
    assert!(
        matches!(refused, VerifyError::Replay { ref from, ts } if from == "dev-a" && ts == NOW)
    );
    assert_eq!(refused.category(), Category::Refused);
}

#[test]
fn one_millisecond_can_carry_several_different_payloads() {
    let key = NodeKey::generate().expect("key");
    let peers = peers_with("dev-a", &key);
    let mut guard = ReplayGuard::new();

    // A message and its answer, or two messages minted in the same millisecond: same
    // `ts`, different bodies, all legitimate.
    for n in 0..3 {
        let signed = message(&key, "dev-a", "dev-b", NOW, json!({ "n": n }));
        assert!(
            verify_at(&signed, "dev-b", &peers, &mut guard, NOW).is_ok(),
            "body {n} at the same millisecond"
        );
    }
    // …and the first one is still a replay if it comes back.
    let first = message(&key, "dev-a", "dev-b", NOW, json!({ "n": 0 }));
    assert!(matches!(
        verify_at(&first, "dev-b", &peers, &mut guard, NOW).expect_err("again"),
        VerifyError::Replay { .. }
    ));
}

#[test]
fn advancing_the_mark_discards_the_old_set() {
    let key = NodeKey::generate().expect("key");
    let peers = peers_with("dev-a", &key);
    let mut guard = ReplayGuard::new();

    let early = NOW;
    let later = NOW + 1_000;
    let at_early = message(&key, "dev-a", "dev-b", early, json!({ "when": "early" }));
    assert!(verify_at(&at_early, "dev-b", &peers, &mut guard, early).is_ok());
    assert_eq!(guard.high_water("dev-a"), Some(early));

    // The mark advances, and the early timestamp is behind it.
    let at_later = message(&key, "dev-a", "dev-b", later, json!({ "when": "later" }));
    assert!(verify_at(&at_later, "dev-b", &peers, &mut guard, later).is_ok());
    assert_eq!(guard.high_water("dev-a"), Some(later));
    assert!(matches!(
        verify_at(&at_early, "dev-b", &peers, &mut guard, later).expect_err("behind the mark"),
        VerifyError::Replay { .. }
    ));

    // The same payload that was seen at the *old* timestamp is accepted at the new one:
    // the set did not follow the mark, it was discarded with it.
    let same_body_later = message(
        &key,
        "dev-a",
        "dev-b",
        later + 1,
        json!({ "when": "early" }),
    );
    assert!(verify_at(&same_body_later, "dev-b", &peers, &mut guard, later).is_ok());
}

#[test]
fn a_key_rotation_does_not_reset_the_record() {
    let old_key = NodeKey::generate().expect("key");
    let new_key = NodeKey::generate().expect("key");
    let mut guard = ReplayGuard::new();

    // Before the rotation: one message, accepted, so the peer's mark is at NOW.
    let before = message(&old_key, "dev-a", "dev-b", NOW, json!({ "round": 1 }));
    assert!(verify_at(
        &before,
        "dev-b",
        &peers_with("dev-a", &old_key),
        &mut guard,
        NOW
    )
    .is_ok());

    // The rotation: the same node_id now carries a new key. Both keys are valid during
    // the grey period (decisions §13), which is what `add_key` models.
    let mut rotated = peers_with("dev-a", &old_key);
    rotated.add_key("dev-a", new_key.verifying_key().expect("verifying key"));

    // A message signed by the NEW key, at an OLD timestamp, is refused as a replay —
    // not as an unknown sender and not as a bad signature. That is the whole point: the
    // record belongs to the peer, so a new key cannot reopen a window a new key closed.
    let replayed = message(&new_key, "dev-a", "dev-b", NOW - 1, json!({ "round": 1 }));
    let refused = verify_at(&replayed, "dev-b", &rotated, &mut guard, NOW).expect_err("replay");
    assert!(
        matches!(refused, VerifyError::Replay { .. }),
        "expected a replay refusal, got {refused:?}"
    );
    assert_eq!(guard.high_water("dev-a"), Some(NOW), "the mark survived");

    // …and the new key is otherwise fine: a fresh timestamp is accepted.
    let fresh = message(&new_key, "dev-a", "dev-b", NOW + 10, json!({ "round": 2 }));
    assert!(verify_at(&fresh, "dev-b", &rotated, &mut guard, NOW).is_ok());
}

#[test]
fn the_canonical_bytes_do_not_depend_on_how_the_body_was_written() {
    let key = NodeKey::generate().expect("key");
    let peers = peers_with("dev-a", &key);
    let mut guard = ReplayGuard::new();

    // The same body, built two ways: the signature covers the canonical JSON, so both
    // are the same message to a verifier.
    let one = message(&key, "dev-a", "dev-b", NOW, json!({ "a": 1, "b": 2 }));
    let two = message(&key, "dev-a", "dev-b", NOW, json!({ "b": 2, "a": 1 }));
    assert_eq!(one.canonical_bytes(), two.canonical_bytes());
    assert_eq!(
        one.sig, two.sig,
        "the same bytes sign to the same signature"
    );

    assert!(verify_at(&two, "dev-b", &peers, &mut guard, NOW).is_ok());
    // The first one is now a replay, which is how we know the two are the same message.
    assert!(matches!(
        verify_at(&one, "dev-b", &peers, &mut guard, NOW).expect_err("the same message"),
        VerifyError::Replay { .. }
    ));
}

#[test]
fn the_wire_line_round_trips_and_still_verifies() {
    let key = NodeKey::generate().expect("key");
    let mut guard = ReplayGuard::new();
    let signed = message(&key, "dev-a", "dev-b", NOW, json!({ "wire": true }));

    let line = signed.to_line().expect("line");
    assert!(line.ends_with('\n'), "one line: {line:?}");
    assert_eq!(line.matches('\n').count(), 1);
    assert!(
        line.starts_with(r#"{"v":1,"from":"dev-a","to":"dev-b","ts":"#),
        "the preamble comes before the signature: {line}"
    );

    let parsed = SignedMessage::parse_line(&line).expect("parse");
    assert_eq!(parsed, signed);
    assert!(verify_at(
        &parsed,
        "dev-b",
        &peers_with("dev-a", &key),
        &mut guard,
        NOW
    )
    .is_ok());

    // A signature that is not 64 bytes of base64url is refused at parse time.
    let broken = line.replace(&signed.sig, "AAAA");
    assert!(SignedMessage::parse_line(&broken).is_err());
}

#[test]
fn the_seen_set_is_keyed_by_the_payload_and_not_by_the_timestamp_alone() {
    let mut guard = ReplayGuard::new();
    let body = json!({ "x": 1 });
    let hash = body_hash(&body);
    assert_eq!(hash, body_hash(&body), "the hash is stable");
    assert_ne!(hash, body_hash(&json!({ "x": 2 })));

    assert!(guard.accept("dev-a", NOW, &hash, NOW).is_ok());
    assert!(guard.accept("dev-a", NOW, &hash, NOW).is_err());
    assert!(guard
        .accept("dev-a", NOW, &body_hash(&json!({ "x": 2 })), NOW)
        .is_ok());
    assert_eq!(guard.high_water("dev-a"), Some(NOW));
}
