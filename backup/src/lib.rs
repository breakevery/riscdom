//! `riscdom-backup` — the portability tool ([backup.md](../../docs/backup.md), v1.0 M7e).
//!
//! **This batch (AV-1) exports a node's data directory** as one encrypted package with a root
//! manifest: `settings.json`, `sessions.db`, `token`, `node.key`, `peers.json` and `rooms.json`
//! ([backup.md](../../docs/backup.md) §1.1). The **audit store** and the **snapshots** — the second
//! root, the workspace's `.riscdom/` (§1.2) — and the **keyring** entries (§1.4) are AV-2; this crate
//! exports neither yet, and says so in the manifest rather than pretending otherwise.
//!
//! The package is a gzipped tar (`manifest.json` plus `data-dir/*`) sealed with **AES-256-GCM** under
//! a key derived from the operator's passphrase with **PBKDF2-HMAC-SHA256** (§2). The passphrase is
//! never written to disk, never printed, and never a command-line argument.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

/// The package's first eight bytes: a magic and a format byte, so a reader can refuse a file that is
/// not one of ours before it does anything else.
pub const PACKAGE_MAGIC: &[u8; 8] = b"RDBAK1\0\0";

/// The manifest's format version, separate from the magic so the header can stay fixed.
pub const FORMAT_VERSION: u32 = 1;

/// The package's default file extension.
pub const PACKAGE_EXTENSION: &str = "rdbak";

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = PACKAGE_MAGIC.len() + SALT_LEN + NONCE_LEN + 4;
const KEY_LEN: usize = 32;

/// PBKDF2 rounds. OWASP's 2023 floor for PBKDF2-HMAC-SHA256; the manifest records nothing here
/// because the header carries it, so a future batch can raise it without breaking old packages.
const PBKDF2_ITERATIONS: u32 = 210_000;

/// The **root** name a data-directory entry is recorded under in the manifest (§1.1).
pub const ROOT_DATA_DIR: &str = "data-dir";

/// The data directory's files that have no home in `host_core::paths`
/// ([backup.md](../../docs/backup.md) §1.1). Each name is the owner's constant —
/// `server::token::TOKEN_FILE`, `net::identity::NODE_KEY_FILE`, `net::peers::PEERS_FILE`,
/// `net::rooms::ROOMS_FILE` — repeated here because this crate deliberately does not depend on
/// `server` or `net`, and a file name is a wire fact, not a rule that drifts.
pub const TOKEN_FILE: &str = "token";
/// See [`TOKEN_FILE`]: `net::identity::NODE_KEY_FILE`.
pub const NODE_KEY_FILE: &str = "node.key";
/// See [`TOKEN_FILE`]: `net::peers::PEERS_FILE`.
pub const PEERS_FILE: &str = "peers.json";
/// See [`TOKEN_FILE`]: `net::rooms::ROOMS_FILE`.
pub const ROOMS_FILE: &str = "rooms.json";

/// One file in the package, as the manifest records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Which root the file came from (§1): `data-dir` today, the workspace's `.riscdom/` in AV-2.
    pub root: String,
    /// The file's name inside that root, e.g. `settings.json`.
    pub path: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its SHA-256, lowercase hex.
    pub sha256: String,
    /// The format's marker, when the file has one: `version=2`, `schema_version=1`,
    /// `user_version=1`. `None` for a file that has no marker (the token file).
    pub marker: Option<String>,
}

/// The root manifest: what the package holds, and the little that names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The tool that wrote it.
    pub tool: String,
    /// [`FORMAT_VERSION`].
    pub format: u32,
    /// The moment of export, epoch milliseconds — the unit the rest of the project uses.
    pub exported_at_ms: i64,
    /// The node's `node_id`, which is its device name ([`agent::identity::device`]).
    pub node_id: String,
    /// Every file the package carries.
    pub entries: Vec<ManifestEntry>,
    /// What the tool could **not** derive and therefore could not carry — the keyring entries
    /// ([backup.md](../../docs/backup.md) §1.4). **Empty in AV-1**: the keyring is AV-2, and until
    /// then this crate carries no credential at all, which the report says out loud rather than
    /// leaving a reader to assume the package is complete.
    #[serde(default)]
    pub not_derived: Vec<String>,
}

/// An exported package: the bytes to write, and the manifest that names what is in them.
#[derive(Debug, Clone)]
pub struct Exported {
    /// The sealed package — the `.rdbak` file, exactly as it goes to disk.
    pub bytes: Vec<u8>,
    /// The manifest the package carries, so a caller can report what was taken.
    pub manifest: Manifest,
}

/// Why an export or a read could not be done.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// The filesystem refused.
    #[error("the filesystem refused: {0}")]
    Io(#[from] std::io::Error),
    /// A JSON document could not be written or read.
    #[error("a manifest or a JSON file could not be read: {0}")]
    Json(#[from] serde_json::Error),
    /// The archive could not be built.
    #[error("the archive could not be built: {0}")]
    Archive(String),
    /// The OS random source refused.
    #[error("the OS random source failed")]
    Random,
    /// The cipher refused (an internal error, never a bad passphrase).
    #[error("the cipher refused to run")]
    Cipher,
    /// The passphrase was wrong, or the package was altered — the two are one answer on purpose,
    /// because an authenticated cipher cannot tell them apart and should not pretend to.
    #[error("the passphrase is wrong, or the package was altered")]
    PassphraseOrTampered,
    /// The bytes are not a package this build can read.
    #[error("not a readable riscdom-backup package: {0}")]
    Malformed(String),
}

/// A file found in the data directory, with the manifest entry it produced.
struct Collected {
    entry: ManifestEntry,
    path: PathBuf,
}

/// Export a node's data directory as one sealed package.
///
/// `data_dir` is the directory a node keeps `settings.json`, `sessions.db`, `token`, `node.key`,
/// `peers.json` and `rooms.json` in; a file that is absent is skipped, not an error (a node that
/// never configured networking has no `node.key`, and one that never served has no `token`).
pub fn export(data_dir: impl AsRef<Path>, passphrase: &[u8]) -> Result<Exported, BackupError> {
    let data_dir = data_dir.as_ref();
    let files = collect(data_dir)?;
    let manifest = Manifest {
        tool: "riscdom-backup".to_string(),
        format: FORMAT_VERSION,
        exported_at_ms: now_ms(),
        node_id: agent::identity::device(),
        entries: files.iter().map(|file| file.entry.clone()).collect(),
        not_derived: Vec::new(),
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let tar = build_tar(&files, &manifest_json)?;
    let gzipped = gzip(&tar)?;
    let bytes = encrypt(&gzipped, passphrase)?;
    Ok(Exported { bytes, manifest })
}

/// The data-directory files, in the order the document lists them.
fn collect(data_dir: &Path) -> Result<Vec<Collected>, BackupError> {
    let mut wanted: Vec<(String, PathBuf)> = Vec::new();
    // `settings.json` and `sessions.db` come from the host's path module, so their names are the
    // runtime's, not this crate's.
    for path in [
        host_core::paths::settings_path_in(data_dir),
        host_core::paths::sessions_db_path_in(data_dir),
    ] {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            wanted.push((name.to_string(), path));
        }
    }
    for name in [TOKEN_FILE, NODE_KEY_FILE, PEERS_FILE, ROOMS_FILE] {
        wanted.push((name.to_string(), data_dir.join(name)));
    }

    let mut out = Vec::new();
    for (name, path) in wanted {
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path)?;
        out.push(Collected {
            entry: ManifestEntry {
                root: ROOT_DATA_DIR.to_string(),
                path: name.clone(),
                size: bytes.len() as u64,
                sha256: hex(&Sha256::digest(&bytes)),
                marker: marker_for(&name, &bytes),
            },
            path,
        });
    }
    Ok(out)
}

/// The format's marker, read the way [api-compatibility.md](../../docs/api-compatibility.md) §6
/// reads it — a version the file itself carries, never a guess.
fn marker_for(name: &str, bytes: &[u8]) -> Option<String> {
    match name {
        "settings.json" => json_marker(bytes, "version"),
        "node.key" | "peers.json" | "rooms.json" => json_marker(bytes, "schema_version"),
        "sessions.db" | "audit.db" => {
            sqlite_user_version(bytes).map(|v| format!("user_version={v}"))
        }
        // The control-plane token has no version at all: one line of hex, shape-checked (§1.1).
        _ => None,
    }
}

/// `key=<n>` from a JSON document's top-level member, or `None` when it is absent or unreadable.
fn json_marker(bytes: &[u8], key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let member = value.get(key)?;
    Some(format!("{key}={member}"))
}

/// SQLite keeps `user_version` as a big-endian `u32` at byte 60 of the file header — the format's
/// own layout, so reading it needs no dependency on a SQLite driver.
fn sqlite_user_version(bytes: &[u8]) -> Option<u32> {
    const OFFSET: usize = 60;
    let raw: [u8; 4] = bytes.get(OFFSET..OFFSET + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(raw))
}

/// `manifest.json` first, then every file under its root — the layout §2 describes, so a restore is
/// a matter of putting things back where they came from.
fn build_tar(files: &[Collected], manifest_json: &[u8]) -> Result<Vec<u8>, BackupError> {
    let mut builder = tar::Builder::new(Vec::new());

    let mut header = tar::Header::new_gnu();
    header.set_size(manifest_json.len() as u64);
    header.set_mode(0o600);
    header.set_mtime(0);
    header.set_cksum();
    builder
        .append_data(&mut header, "manifest.json", manifest_json)
        .map_err(archive_error)?;

    for file in files {
        let mut source = fs::File::open(&file.path)?;
        builder
            .append_file(format!("{ROOT_DATA_DIR}/{}", file.entry.path), &mut source)
            .map_err(archive_error)?;
    }
    builder.into_inner().map_err(BackupError::Io)
}

fn archive_error(error: std::io::Error) -> BackupError {
    BackupError::Archive(error.to_string())
}

/// gzip the tar. Compression comes **before** encryption: an encrypted archive cannot be compressed
/// afterwards, and compressing first costs nothing in secrecy that the cipher was not already
/// giving (the package's length is the only thing a compressor leaks, and it is inside the seal).
fn gzip(data: &[u8]) -> Result<Vec<u8>, BackupError> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

/// Gunzip an archive body (the inverse of [`gzip`]).
fn gunzip(data: &[u8]) -> Result<Vec<u8>, BackupError> {
    use std::io::Read;
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

/// Seal `plaintext` under `passphrase`. The header (magic, salt, nonce, rounds) is the AEAD's
/// additional data, so altering any of it makes the package unreadable rather than merely wrong.
pub fn encrypt(plaintext: &[u8], passphrase: &[u8]) -> Result<Vec<u8>, BackupError> {
    use ring::rand::SecureRandom;
    let rng = ring::rand::SystemRandom::new();
    let mut salt = [0u8; SALT_LEN];
    rng.fill(&mut salt).map_err(|_| BackupError::Random)?;
    let mut nonce = [0u8; NONCE_LEN];
    rng.fill(&mut nonce).map_err(|_| BackupError::Random)?;

    let key_bytes = derive_key(passphrase, &salt, PBKDF2_ITERATIONS)?;
    let sealing = sealing_key(&key_bytes)?;

    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(PACKAGE_MAGIC);
    header.extend_from_slice(&salt);
    header.extend_from_slice(&nonce);
    header.extend_from_slice(&PBKDF2_ITERATIONS.to_le_bytes());

    let mut body = plaintext.to_vec();
    sealing
        .seal_in_place_append_tag(
            ring::aead::Nonce::assume_unique_for_key(nonce),
            ring::aead::Aad::from(&header),
            &mut body,
        )
        .map_err(|_| BackupError::Cipher)?;
    header.extend_from_slice(&body);
    Ok(header)
}

/// Open a package created by [`encrypt`]. A wrong passphrase and an altered file give one answer
/// ([`BackupError::PassphraseOrTampered`]), because an authenticated cipher cannot tell them apart.
pub fn decrypt(package: &[u8], passphrase: &[u8]) -> Result<Vec<u8>, BackupError> {
    if package.len() < HEADER_LEN {
        return Err(BackupError::Malformed("shorter than its own header".into()));
    }
    if &package[..PACKAGE_MAGIC.len()] != PACKAGE_MAGIC {
        return Err(BackupError::Malformed("the magic does not match".into()));
    }
    let mut at = PACKAGE_MAGIC.len();
    let salt = &package[at..at + SALT_LEN];
    at += SALT_LEN;
    let nonce: [u8; NONCE_LEN] = package[at..at + NONCE_LEN]
        .try_into()
        .map_err(|_| BackupError::Malformed("the nonce is the wrong length".into()))?;
    at += NONCE_LEN;
    let iterations: [u8; 4] = package[at..at + 4]
        .try_into()
        .map_err(|_| BackupError::Malformed("the round count is the wrong length".into()))?;
    let iterations = u32::from_le_bytes(iterations);

    let header = &package[..HEADER_LEN];
    let key_bytes = derive_key(passphrase, salt, iterations)?;
    let opening = sealing_key(&key_bytes)?;
    let mut body = package[HEADER_LEN..].to_vec();
    let plaintext = opening
        .open_in_place(
            ring::aead::Nonce::assume_unique_for_key(nonce),
            ring::aead::Aad::from(header),
            &mut body,
        )
        .map_err(|_| BackupError::PassphraseOrTampered)?;
    Ok(plaintext.to_vec())
}

/// Read a package's manifest without unpacking it — the "say what it is holding before writing
/// anything" step of §2 and §5.
pub fn read_manifest(package: &[u8], passphrase: &[u8]) -> Result<Manifest, BackupError> {
    let tar = gunzip(&decrypt(package, passphrase)?)?;
    let mut archive = tar::Archive::new(tar.as_slice());
    for entry in archive.entries().map_err(archive_error)? {
        let mut entry = entry.map_err(archive_error)?;
        let path = entry
            .path()
            .map_err(archive_error)?
            .to_string_lossy()
            .to_string();
        if path == "manifest.json" {
            return Ok(serde_json::from_reader(&mut entry)?);
        }
    }
    Err(BackupError::Malformed(
        "the package carries no manifest".into(),
    ))
}

/// PBKDF2-HMAC-SHA256, the passphrase stretched into the cipher's key.
fn derive_key(
    passphrase: &[u8],
    salt: &[u8],
    iterations: u32,
) -> Result<[u8; KEY_LEN], BackupError> {
    let rounds = NonZeroU32::new(iterations).ok_or(BackupError::Cipher)?;
    let mut key = [0u8; KEY_LEN];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        rounds,
        salt,
        passphrase,
        &mut key,
    );
    Ok(key)
}

fn sealing_key(key_bytes: &[u8; KEY_LEN]) -> Result<ring::aead::LessSafeKey, BackupError> {
    let unbound = ring::aead::UnboundKey::new(&ring::aead::AES_256_GCM, key_bytes)
        .map_err(|_| BackupError::Cipher)?;
    Ok(ring::aead::LessSafeKey::new(unbound))
}

/// Lowercase hex, the spelling the audit chain and the fingerprint use.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Epoch milliseconds, the project's one clock.
fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => since.as_millis() as i64,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("riscdom-backup-{tag}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A minimal but well-formed SQLite header: the 16-byte magic, then `user_version` at 60.
    fn sqlite_header(user_version: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[..16].copy_from_slice(b"SQLite format 3\0");
        bytes[60..64].copy_from_slice(&user_version.to_be_bytes());
        bytes
    }

    fn populated(dir: &Path) {
        fs::write(dir.join("settings.json"), br#"{"version":2,"theme":null}"#).unwrap();
        fs::write(dir.join("sessions.db"), sqlite_header(1)).unwrap();
        fs::write(dir.join("token"), "deadbeef\n").unwrap();
        fs::write(dir.join("node.key"), br#"{"schema_version":1,"kty":"OKP"}"#).unwrap();
        fs::write(
            dir.join("peers.json"),
            br#"{"schema_version":1,"peers":[]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("rooms.json"),
            br#"{"schema_version":1,"rooms":[]}"#,
        )
        .unwrap();
        // Neither of these is node state, so neither may be carried (§1.3).
        fs::write(dir.join("settings.json.bak"), b"old").unwrap();
        fs::write(dir.join("other.txt"), b"not node state").unwrap();
    }

    #[test]
    fn round_trip_recovers_the_plaintext() {
        let sealed = encrypt(b"the plaintext", b"a passphrase").unwrap();
        assert_eq!(&sealed[..PACKAGE_MAGIC.len()], PACKAGE_MAGIC);
        assert_eq!(decrypt(&sealed, b"a passphrase").unwrap(), b"the plaintext");
    }

    #[test]
    fn a_wrong_passphrase_is_refused() {
        let sealed = encrypt(b"the plaintext", b"right").unwrap();
        assert!(matches!(
            decrypt(&sealed, b"wrong"),
            Err(BackupError::PassphraseOrTampered)
        ));
    }

    #[test]
    fn an_altered_package_is_refused() {
        let mut sealed = encrypt(b"the plaintext", b"a passphrase").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert!(matches!(
            decrypt(&sealed, b"a passphrase"),
            Err(BackupError::PassphraseOrTampered)
        ));
    }

    #[test]
    fn a_foreign_file_is_refused_before_anything_else() {
        assert!(matches!(
            decrypt(b"not a package at all, not even close", b"x"),
            Err(BackupError::Malformed(_))
        ));
    }

    #[test]
    fn the_manifest_names_every_file_and_its_marker() {
        let dir = temp_dir("manifest");
        populated(&dir);
        let exported = export(&dir, b"test passphrase").unwrap();

        let by_path = |name: &str| {
            exported
                .manifest
                .entries
                .iter()
                .find(|entry| entry.path == name)
                .unwrap_or_else(|| panic!("{name} is missing from the manifest"))
        };

        assert_eq!(exported.manifest.entries.len(), 6);
        assert_eq!(
            by_path("settings.json").marker.as_deref(),
            Some("version=2")
        );
        assert_eq!(
            by_path("sessions.db").marker.as_deref(),
            Some("user_version=1")
        );
        assert_eq!(
            by_path("node.key").marker.as_deref(),
            Some("schema_version=1")
        );
        assert_eq!(by_path("token").marker, None);
        for name in [
            "settings.json",
            "sessions.db",
            "token",
            "node.key",
            "peers.json",
            "rooms.json",
        ] {
            let on_disk = fs::read(dir.join(name)).unwrap();
            assert_eq!(by_path(name).size, on_disk.len() as u64, "{name} size");
            assert_eq!(
                by_path(name).sha256,
                hex(&Sha256::digest(&on_disk)),
                "{name} hash"
            );
        }

        // §1.3: the escape hatch and the stray file are not node state.
        assert!(exported
            .manifest
            .entries
            .iter()
            .all(|entry| entry.path != "settings.json.bak"));
        assert!(exported
            .manifest
            .entries
            .iter()
            .all(|entry| entry.path != "other.txt"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_payload_carries_every_file_byte_for_byte() {
        let dir = temp_dir("payload");
        populated(&dir);
        let exported = export(&dir, b"test passphrase").unwrap();

        let tar_bytes = gunzip(&decrypt(&exported.bytes, b"test passphrase").unwrap()).unwrap();
        let mut archive = tar::Archive::new(tar_bytes.as_slice());
        let mut seen_manifest = false;
        let mut compared = 0;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().to_string();
            let mut body = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut body).unwrap();
            if path == "manifest.json" {
                seen_manifest = true;
                let parsed: Manifest = serde_json::from_slice(&body).unwrap();
                assert_eq!(parsed, exported.manifest);
                continue;
            }
            let name = path
                .strip_prefix(&format!("{ROOT_DATA_DIR}/"))
                .unwrap_or_else(|| panic!("unexpected archive entry {path}"));
            assert_eq!(body, fs::read(dir.join(name)).unwrap(), "{name} differs");
            compared += 1;
        }
        assert!(seen_manifest, "the archive must carry its manifest");
        assert_eq!(compared, 6);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_empty_directory_exports_an_empty_manifest() {
        let dir = temp_dir("empty");
        let exported = export(&dir, b"test passphrase").unwrap();
        assert!(exported.manifest.entries.is_empty());
        assert_eq!(exported.manifest.format, FORMAT_VERSION);
        assert!(!exported.manifest.node_id.is_empty());
        assert!(exported.manifest.not_derived.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn absent_files_are_skipped_not_failed() {
        let dir = temp_dir("partial");
        fs::write(dir.join("settings.json"), br#"{"version":2}"#).unwrap();
        let exported = export(&dir, b"test passphrase").unwrap();
        assert_eq!(exported.manifest.entries.len(), 1);
        assert_eq!(exported.manifest.entries[0].path, "settings.json");
        fs::remove_dir_all(&dir).ok();
    }
}
