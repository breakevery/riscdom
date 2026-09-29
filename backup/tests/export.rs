//! An end-to-end export over a real data directory and a real workspace, through the crate's public
//! API only.
//!
//! The unit tests inside the crate compare the payload byte for byte; this one takes the outside
//! view on purpose: it builds the two roots the way a node would, exports them with an **in-memory**
//! keyring (so it never touches the machine's credential store), and reads the manifest back out of
//! the sealed package.

use riscdom_backup::{
    export_with, read_manifest, InMemoryKeyring, KeyringBackend, PACKAGE_MAGIC, ROOT_DATA_DIR,
    ROOT_KEYRING, ROOT_WORKSPACE,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "riscdom-backup-it-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A SQLite file header with `user_version` set — enough for the marker, nothing more.
fn sqlite_header(user_version: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[..16].copy_from_slice(b"SQLite format 3\0");
    bytes[60..64].copy_from_slice(&user_version.to_be_bytes());
    bytes
}

fn node_data_dir(dir: &Path) {
    fs::write(
        dir.join("settings.json"),
        br#"{"version":2,"llm_configs":{"local":{"provider_id":"deepseek"}}}"#,
    )
    .unwrap();
    fs::write(dir.join("sessions.db"), sqlite_header(1)).unwrap();
    fs::write(dir.join("token"), "0123456789abcdef\n").unwrap();
    fs::write(dir.join("node.key"), br#"{"schema_version":1}"#).unwrap();
    fs::write(dir.join("peers.json"), br#"{"schema_version":1}"#).unwrap();
    fs::write(dir.join("rooms.json"), br#"{"schema_version":1}"#).unwrap();
}

fn node_workspace(dir: &Path) {
    // The audit store is exercised by the crate's own unit tests (it needs `rusqlite`, which an
    // integration test cannot link); here the workspace root is carried by its snapshots.
    let nested = dir
        .join(".riscdom")
        .join("snapshots")
        .join("local")
        .join("local-1-1");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("state.mig"), b"a snapshot").unwrap();
}

#[test]
fn a_whole_node_exports_as_one_sealed_package() {
    let data_dir = temp_dir("export-data");
    let workspace = temp_dir("export-ws");
    node_data_dir(&data_dir);
    node_workspace(&workspace);

    let keyring = InMemoryKeyring::new();
    keyring
        .set(
            "com.breakevery.riscdom",
            "llm-api-key:local:deepseek",
            "a-secret",
        )
        .unwrap();
    let phrase = b"a test passphrase";

    let exported = export_with(&data_dir, &workspace, phrase, &keyring).unwrap();

    // The package is one file, and it announces itself.
    assert_eq!(&exported.bytes[..PACKAGE_MAGIC.len()], PACKAGE_MAGIC);
    assert!(exported.bytes.len() > PACKAGE_MAGIC.len());

    // All three roots are present: the data directory, the workspace, and the keyring.
    let roots: Vec<&str> = exported
        .manifest
        .entries
        .iter()
        .map(|entry| entry.root.as_str())
        .collect();
    assert!(roots.contains(&ROOT_DATA_DIR));
    assert!(roots.contains(&ROOT_WORKSPACE));
    assert!(roots.contains(&ROOT_KEYRING));

    // Reading it back needs the passphrase and yields the same manifest.
    let read = read_manifest(&exported.bytes, phrase).unwrap();
    assert_eq!(read, exported.manifest);

    // Under the seal is a gzip stream — the archive body, compressed before it was encrypted.
    let plaintext = riscdom_backup::decrypt(&exported.bytes, phrase).unwrap();
    assert_eq!(&plaintext[..2], &[0x1f, 0x8b]);

    // What could not be carried is stated, never hidden.
    assert!(exported
        .manifest
        .not_derived
        .iter()
        .any(|line| line.starts_with("unnameable:")));

    fs::remove_dir_all(&data_dir).ok();
    fs::remove_dir_all(&workspace).ok();
}

#[test]
fn a_wrong_passphrase_reads_nothing() {
    let data_dir = temp_dir("wrong-phrase-data");
    let workspace = temp_dir("wrong-phrase-ws");
    node_data_dir(&data_dir);
    let exported = export_with(
        &data_dir,
        &workspace,
        b"the right one",
        &InMemoryKeyring::new(),
    )
    .unwrap();
    assert!(read_manifest(&exported.bytes, b"the wrong one").is_err());
    fs::remove_dir_all(&data_dir).ok();
    fs::remove_dir_all(&workspace).ok();
}
