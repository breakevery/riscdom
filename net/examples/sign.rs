//! Signing and its six refusals, proved offline (v1.0 M4a).
//!
//! ```text
//! cargo run -p net --example sign -- --self-test
//! ```
//!
//! No network, no server: two in-memory keys, a known-peer table, and a replay guard.
//! `scripts/gate.sh` runs this beside the other example-level proofs.

use net::{
    verify_at, Category, NodeKey, PeerKeys, ReplayGuard, SignedMessage, VerifyError,
    REPLAY_WINDOW_AHEAD_MS, REPLAY_WINDOW_BACK_MS,
};
use serde_json::json;

const NOW: i64 = 1_700_000_000_000;

fn check(what: &str, ok: bool, detail: String) {
    println!("{}  {what}: {detail}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}

fn category_of(error: &VerifyError) -> Category {
    error.category()
}

fn self_test() {
    let alice = NodeKey::generate().expect("alice");
    let bob = NodeKey::generate().expect("bob");
    let mut peers = PeerKeys::new();
    peers.insert("alice", [alice.verifying_key().expect("public")]);

    let sign = |body: serde_json::Value| {
        SignedMessage::sign(&alice, "alice", "bob", NOW, body).expect("sign")
    };

    // 1. The happy path.
    let mut guard = ReplayGuard::new();
    let verified = verify_at(
        &sign(json!({ "hello": "bob" })),
        "bob",
        &peers,
        &mut guard,
        NOW,
    )
    .expect("verify");
    check(
        "a signed message verifies and names its sender",
        verified.from == "alice" && verified.to == "bob" && verified.ts == NOW,
        format!("{} → {} at {}", verified.from, verified.to, verified.ts),
    );

    // 2. Step 1 — an unknown sender.
    let stranger = NodeKey::generate().expect("stranger");
    let from_stranger =
        SignedMessage::sign(&stranger, "mallory", "bob", NOW, json!({})).expect("sign");
    let error =
        verify_at(&from_stranger, "bob", &peers, &mut guard, NOW).expect_err("unknown sender");
    check(
        "step 1 refuses an unknown sender (refused)",
        matches!(error, VerifyError::UnknownSender(_)) && category_of(&error) == Category::Refused,
        error.to_string(),
    );

    // 3. Step 2 — a tampered payload.
    let mut tampered = sign(json!({ "amount": 1 }));
    tampered.body = json!({ "amount": 2 });
    let error = verify_at(&tampered, "bob", &peers, &mut guard, NOW).expect_err("tampered");
    check(
        "step 2 refuses a tampered payload (invalid)",
        matches!(error, VerifyError::BadSignature(_)) && category_of(&error) == Category::Invalid,
        error.to_string(),
    );

    // 4. Step 3 — a protocol version we do not speak.
    let newer = SignedMessage::sign_with_version(
        net::PROTOCOL_VERSION + 1,
        &alice.signing_key().expect("signing key"),
        "alice",
        "bob",
        NOW,
        json!({}),
    );
    let error = verify_at(&newer, "bob", &peers, &mut guard, NOW).expect_err("newer version");
    check(
        "step 3 refuses a newer protocol version (invalid)",
        matches!(error, VerifyError::UnsupportedVersion { .. })
            && category_of(&error) == Category::Invalid,
        error.to_string(),
    );

    // 5. Step 4 — addressed to somebody else.
    let elsewhere = SignedMessage::sign(&alice, "alice", "carol", NOW, json!({})).expect("sign");
    let error = verify_at(&elsewhere, "bob", &peers, &mut guard, NOW).expect_err("elsewhere");
    check(
        "step 4 refuses a message addressed elsewhere (invalid)",
        matches!(error, VerifyError::NotAddressedToUs { .. })
            && category_of(&error) == Category::Invalid,
        error.to_string(),
    );

    // 6. Step 5 — outside the window, in both directions.
    let stale = SignedMessage::sign(
        &alice,
        "alice",
        "bob",
        NOW - REPLAY_WINDOW_BACK_MS - 1,
        json!({}),
    )
    .expect("sign");
    let error = verify_at(&stale, "bob", &peers, &mut guard, NOW).expect_err("stale");
    let stale_ok =
        matches!(error, VerifyError::Stale { .. }) && category_of(&error) == Category::Network;

    let future = SignedMessage::sign(
        &alice,
        "alice",
        "bob",
        NOW + REPLAY_WINDOW_AHEAD_MS + 1,
        json!({}),
    )
    .expect("sign");
    let error = verify_at(&future, "bob", &peers, &mut guard, NOW).expect_err("future");
    let future_ok =
        matches!(error, VerifyError::Future { .. }) && category_of(&error) == Category::Network;
    check(
        "step 5 refuses a stale and a future timestamp (network)",
        stale_ok && future_ok,
        format!("{stale_ok} / {future_ok}"),
    );

    // 7. Step 6 — the same message a second time.
    let once = sign(json!({ "once": true }));
    assert!(verify_at(&once, "bob", &peers, &mut guard, NOW).is_ok());
    let error = verify_at(&once, "bob", &peers, &mut guard, NOW).expect_err("replay");
    check(
        "step 6 refuses a replay (refused)",
        matches!(error, VerifyError::Replay { .. }) && category_of(&error) == Category::Refused,
        error.to_string(),
    );

    // 8. A key rotation does not reset the peer's mark.
    let rotated = NodeKey::generate().expect("rotated");
    peers.add_key("alice", rotated.verifying_key().expect("public"));
    let by_new_key_old_ts =
        SignedMessage::sign(&rotated, "alice", "bob", NOW - 1, json!({ "round": 1 }))
            .expect("sign");
    let error = verify_at(&by_new_key_old_ts, "bob", &peers, &mut guard, NOW).expect_err("replay");
    check(
        "a rotation does not reopen a window the old key closed",
        matches!(error, VerifyError::Replay { .. }),
        error.to_string(),
    );

    // 9. The wire form is one line, and it round-trips.
    let line = sign(json!({ "wire": true })).to_line().expect("line");
    let parsed = SignedMessage::parse_line(&line).expect("parse");
    let mut fresh = ReplayGuard::new();
    check(
        "a message survives the wire form",
        line.ends_with('\n')
            && line.matches('\n').count() == 1
            && verify_at(&parsed, "bob", &peers, &mut fresh, NOW).is_ok(),
        line.trim().to_string(),
    );

    let _ = bob;
    println!("net signing self-test: OK");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        self_test();
        return;
    }
    println!("usage: cargo run -p net --example sign -- --self-test");
    println!("signs a message between two in-memory keys and proves all six refusals.");
}
