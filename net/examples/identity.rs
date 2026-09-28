//! The node key, proved offline: mint, write, read back, refuse what must be refused.
//!
//! ```text
//! cargo run -p net --example identity -- --self-test
//! ```
//!
//! `--self-test` is the shape the gate uses for the other example-level proofs
//! (`worker/examples/remote_executor.rs`), and `scripts/gate.sh` runs it beside them.
//! No network, no server, no QEMU: it works in a scratch directory under the system
//! temp and checks the file the protocol freezes.

use net::{NodeKey, VersionedLoad, NODE_KEY_FILE};
use std::path::PathBuf;

fn scratch_dir(tag: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "riscdom-net-identity-{tag}-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn check(what: &str, ok: bool, detail: String) {
    println!("{}  {what}: {detail}", if ok { "PASS" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}

fn self_test() {
    // 1. A key is minted only when networking is configured.
    let off = scratch_dir("off");
    check(
        "a node with no networking mints nothing",
        matches!(NodeKey::load_or_create_in(&off, false), Ok(None))
            && !off.join(NODE_KEY_FILE).exists(),
        format!("{}", off.join(NODE_KEY_FILE).display()),
    );

    let dir = scratch_dir("on");
    let minted = NodeKey::load_or_create_in(&dir, true)
        .expect("mint")
        .expect("a key");
    let path = dir.join(NODE_KEY_FILE);
    check(
        "the first start with networking mints one file",
        path.is_file(),
        path.display().to_string(),
    );

    // 2. The bytes are the frozen shape: `schema_version` first, one JWK.
    let raw = std::fs::read_to_string(&path).expect("read");
    check(
        "schema_version is the first member",
        raw.starts_with("{\n  \"schema_version\": 1,"),
        raw.lines().next().unwrap_or_default().to_string(),
    );
    check(
        "the file is an OKP/Ed25519 JWK",
        raw.contains("\"kty\": \"OKP\"") && raw.contains("\"crv\": \"Ed25519\""),
        "kty/crv".to_string(),
    );

    // 3. It reads back as the same key, and the two halves belong together.
    let read = match NodeKey::load_in(&dir).expect("load") {
        VersionedLoad::Current(key) => key,
        other => {
            check(
                "the file reads back as current",
                false,
                format!("{other:?}"),
            );
            return;
        }
    };
    check(
        "the key survives a round trip",
        read == minted && read.signing_key().is_ok(),
        read.short_fingerprint(),
    );

    // 4. A second start reads, and never mints over it.
    let again = NodeKey::load_or_create_in(&dir, true)
        .expect("read")
        .expect("a key");
    check(
        "the second start reuses the first key",
        again == minted,
        again.short_fingerprint(),
    );
    check(
        "minting refuses to overwrite",
        NodeKey::save_new_in(&dir, &minted).is_err(),
        "create_new".to_string(),
    );

    // 5. A newer file is refused, not half-read.
    let newer = scratch_dir("too-new");
    std::fs::write(
        newer.join(NODE_KEY_FILE),
        r#"{"schema_version": 2, "kty": "OKP", "crv": "Ed25519", "x": "AA", "d": "AA"}"#,
    )
    .expect("write");
    check(
        "a newer file is refused",
        matches!(
            NodeKey::load_in(&newer).expect("load"),
            VersionedLoad::TooNew { found: 2 }
        ),
        "TooNew { found: 2 }".to_string(),
    );

    // 6. Reading a missing file answers Missing, and creates nothing.
    let none = scratch_dir("missing");
    check(
        "a missing file is missing, and a read mints nothing",
        matches!(
            NodeKey::load_in(&none).expect("load"),
            VersionedLoad::Missing
        ) && !none.join(NODE_KEY_FILE).exists(),
        "Missing".to_string(),
    );

    // 7. Owner-only, as the protocol requires.
    let metadata = std::fs::metadata(&path).expect("metadata");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        check(
            "the key file is readable only by its owner",
            metadata.permissions().mode() & 0o777 == 0o600,
            format!("{:o}", metadata.permissions().mode() & 0o777),
        );
    }
    #[cfg(windows)]
    {
        let listing = std::process::Command::new("icacls")
            .arg(&path)
            .output()
            .expect("icacls");
        let text = String::from_utf8_lossy(&listing.stdout);
        let principals = text
            .split_whitespace()
            .filter(|token| token.contains(":("))
            .count();
        check(
            "the key file's ACL lists one principal",
            principals == 1,
            format!("{principals} principal(s)"),
        );
    }
    let _ = metadata;

    println!("net identity self-test: OK");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--self-test") {
        self_test();
        return;
    }
    println!("usage: cargo run -p net --example identity -- --self-test");
    println!("mints a node key in a scratch directory and proves the file's shape.");
}
