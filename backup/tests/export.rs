//! An end-to-end export over a real data directory, through the crate's public API only.
//!
//! The unit tests inside the crate already compare the payload byte for byte; this one takes the
//! outside view on purpose — it builds a directory the way a node would, exports it, and reads the
//! manifest back out of the sealed package.

use riscdom_backup::{export, read_manifest, PACKAGE_MAGIC};
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
    fs::write(dir.join("settings.json"), br#"{"version":2}"#).unwrap();
    fs::write(dir.join("sessions.db"), sqlite_header(1)).unwrap();
    fs::write(dir.join("token"), "0123456789abcdef\n").unwrap();
    fs::write(dir.join("node.key"), br#"{"schema_version":1}"#).unwrap();
    fs::write(dir.join("peers.json"), br#"{"schema_version":1}"#).unwrap();
    fs::write(dir.join("rooms.json"), br#"{"schema_version":1}"#).unwrap();
}

#[test]
fn a_node_directory_exports_as_one_sealed_package() {
    let dir = temp_dir("export");
    node_data_dir(&dir);
    let passphrase = b"a test passphrase";

    let exported = export(&dir, passphrase).unwrap();

    // The package is one file, and it announces itself.
    assert_eq!(&exported.bytes[..PACKAGE_MAGIC.len()], PACKAGE_MAGIC);
    assert!(exported.bytes.len() > PACKAGE_MAGIC.len());

    // The manifest names all six files, and the record of what was *not* carried is there (§1.4).
    assert_eq!(exported.manifest.entries.len(), 6);
    assert_eq!(exported.manifest.format, 1);
    assert!(!exported.manifest.node_id.is_empty());
    assert!(exported
        .manifest
        .entries
        .iter()
        .all(|entry| entry.root == "data-dir"));
    assert!(
        exported.manifest.not_derived.is_empty(),
        "AV-1 carries no keyring entries"
    );

    // Reading it back needs the passphrase and yields the same manifest.
    let read = read_manifest(&exported.bytes, passphrase).unwrap();
    assert_eq!(read, exported.manifest);

    // Under the seal is a gzip stream — the archive body, compressed before it was encrypted.
    let plaintext = riscdom_backup::decrypt(&exported.bytes, passphrase).unwrap();
    assert_eq!(&plaintext[..2], &[0x1f, 0x8b]);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_wrong_passphrase_reads_nothing() {
    let dir = temp_dir("wrong-passphrase");
    node_data_dir(&dir);
    let exported = export(&dir, b"the right one").unwrap();
    assert!(read_manifest(&exported.bytes, b"the wrong one").is_err());
    fs::remove_dir_all(&dir).ok();
}
