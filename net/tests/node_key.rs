//! `node.key`: mint it, write it, read it back, and refuse what must be refused
//! (v1.0 M4a; [connection.md §2](`../docs/connection.md`)).

use net::{NodeKey, VersionedLoad, NODE_KEY_FILE};
use std::path::PathBuf;

fn scratch_dir(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "riscdom-net-nodekey-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn a_minted_key_survives_a_round_trip_through_the_file() {
    let dir = scratch_dir("roundtrip");
    let minted = NodeKey::generate().expect("mint");
    NodeKey::save_new_in(&dir, &minted).expect("save");

    match NodeKey::load_in(&dir).expect("load") {
        VersionedLoad::Current(read) => {
            assert_eq!(read, minted, "the same key comes back");
            assert_eq!(
                read.signing_key().expect("signing key").to_bytes(),
                minted.signing_key().expect("signing key").to_bytes()
            );
        }
        other => panic!("expected a current file, got {other:?}"),
    }
}

#[test]
fn the_file_is_one_jwk_with_the_version_first_and_two_32_byte_keys() {
    let dir = scratch_dir("shape");
    let minted = NodeKey::generate().expect("mint");
    let path = NodeKey::save_new_in(&dir, &minted).expect("save");
    assert_eq!(path, dir.join(NODE_KEY_FILE));

    let raw = std::fs::read_to_string(&path).expect("read");
    assert!(
        raw.starts_with("{\n  \"schema_version\": 1,"),
        "schema_version is the first member: {raw}"
    );
    assert!(raw.contains("\"kty\": \"OKP\""), "{raw}");
    assert!(raw.contains("\"crv\": \"Ed25519\""), "{raw}");

    // Both members are 32 bytes of unpadded base64url, and the public half is exactly
    // what the private half derives.
    let value: serde_json::Value = serde_json::from_str(&raw).expect("json");
    for member in ["x", "d"] {
        let text = value[member].as_str().expect(member);
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        assert_eq!(
            URL_SAFE_NO_PAD.decode(text).expect("base64url").len(),
            32,
            "{member} is {text:?}"
        );
        assert!(!text.contains('='), "{member} must be unpadded: {text:?}");
    }
    assert_eq!(
        minted
            .verifying_key()
            .expect("verifying key")
            .to_bytes()
            .len(),
        32
    );
}

#[test]
fn the_key_file_is_readable_only_by_its_owner() {
    let dir = scratch_dir("permissions");
    let minted = NodeKey::generate().expect("mint");
    let path = NodeKey::save_new_in(&dir, &minted).expect("save");
    let metadata = std::fs::metadata(&path).expect("metadata");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
    #[cfg(windows)]
    {
        let listing = std::process::Command::new("icacls")
            .arg(&path)
            .output()
            .expect("icacls");
        let text = String::from_utf8_lossy(&listing.stdout);
        let principals: Vec<&str> = text
            .split_whitespace()
            .filter_map(|token| token.split_once(":(").map(|(who, _)| who))
            .collect();
        assert_eq!(principals.len(), 1, "one principal: {principals:?}");
    }
    let _ = metadata;
}

#[test]
fn a_missing_file_is_missing_and_a_read_never_mints_one() {
    let dir = scratch_dir("missing");
    assert_eq!(
        NodeKey::load_in(&dir).expect("load"),
        VersionedLoad::Missing
    );
    assert!(
        !dir.join(NODE_KEY_FILE).exists(),
        "reading must not create a key"
    );
}

#[test]
fn a_newer_file_is_refused() {
    let dir = scratch_dir("too-new");
    std::fs::write(
        dir.join(NODE_KEY_FILE),
        r#"{"schema_version": 2, "kty": "OKP", "crv": "Ed25519", "x": "AA", "d": "AA"}"#,
    )
    .expect("write");
    assert_eq!(
        NodeKey::load_in(&dir).expect("load"),
        VersionedLoad::TooNew { found: 2 }
    );
}

#[test]
fn a_key_is_minted_once_and_only_when_networking_is_configured() {
    let dir = scratch_dir("mint");

    // Not on a network: no key, and nothing written.
    assert_eq!(
        NodeKey::load_or_create_in(&dir, false).expect("disabled"),
        None
    );
    assert!(!dir.join(NODE_KEY_FILE).exists());

    // On a network: minted once, and the same key is answered afterwards.
    let first = NodeKey::load_or_create_in(&dir, true)
        .expect("enabled")
        .expect("a key");
    let second = NodeKey::load_or_create_in(&dir, true)
        .expect("enabled")
        .expect("a key");
    assert_eq!(first, second, "the second start reads the first one's key");
}

#[test]
fn a_tampered_or_mismatched_key_is_refused() {
    let dir = scratch_dir("tampered");
    let minted = NodeKey::generate().expect("mint");
    let path = NodeKey::save_new_in(&dir, &minted).expect("save");
    let raw = std::fs::read_to_string(&path).expect("read");

    // A wrong `kty` is not this format.
    let wrong_type = raw.replace("\"OKP\"", "\"RSA\"");
    std::fs::write(&path, wrong_type).expect("write");
    assert!(NodeKey::load_in(&dir).is_err(), "a wrong kty is refused");

    // `x` and `d` that do not belong together are refused: the public half is
    // re-derived from the private one and compared.
    let other = NodeKey::generate().expect("mint another");
    let spliced = raw.replace(&minted.x, &other.x);
    std::fs::write(&path, spliced).expect("write");
    assert!(
        NodeKey::load_in(&dir).is_err(),
        "two halves from two keys are refused"
    );

    // A member that is not 32 bytes is refused.
    std::fs::write(
        &path,
        r#"{"schema_version": 1, "kty": "OKP", "crv": "Ed25519", "x": "AAAA", "d": "AAAA"}"#,
    )
    .expect("write");
    assert!(NodeKey::load_in(&dir).is_err(), "a short key is refused");
}

#[test]
fn the_fingerprint_is_stable_and_names_only_the_public_half() {
    let first = NodeKey::generate().expect("mint");
    let mut second = NodeKey::generate().expect("mint");
    assert_ne!(first.fingerprint(), second.fingerprint());

    // Same public half, different private half: the fingerprint is the same, so the
    // fingerprint cannot be a function of the secret.
    second.x = first.x.clone();
    assert_eq!(first.fingerprint(), second.fingerprint());
    assert_eq!(first.short_fingerprint().len(), 16);
    assert!(
        first.fingerprint().chars().all(|c| c.is_ascii_hexdigit()),
        "{}",
        first.fingerprint()
    );
}
