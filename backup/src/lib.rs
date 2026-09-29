//! `riscdom-backup` — the portability tool ([backup.md](../../docs/backup.md), v1.0 M7e).
//!
//! **AV-1** exported a node's **data directory** and sealed it. **AV-2** completes the package with
//! the remaining two roots of §1:
//!
//! - the **audit store** (`<workspace>/.riscdom/audit.db`), taken through **SQLite's own consistent
//!   path** (`VACUUM INTO`) because a byte copy of a WAL database can miss frames still in `-wal`;
//! - the **snapshots** (`<workspace>/.riscdom/snapshots/<device>/<id>/`), walked and carried whole;
//! - the **credentials** the OS keyring holds — **derived**, never enumerated, because `keyring` v3
//!   has no listing API. The account names come from `settings.json` (§1.4), and whatever cannot be
//!   derived is **reported in the manifest's `not_derived` list**, never silently missed.
//!
//! The container is unchanged from AV-1 (tar → gzip → a sealed AES-256-GCM body), so a format-1
//! package only gained entries.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

pub use host_core::keyring::{InMemoryKeyring, KeyringBackend};

/// The package's first eight bytes: a magic and a format byte, so a reader can refuse a file that is
/// not one of ours before it does anything else.
pub const PACKAGE_MAGIC: &[u8; 8] = b"RDBAK1\0\0";

/// The manifest's format version. AV-2 added entries and roots but no new shape, so it stays **1**:
/// a reader that knew format 1 reads an AV-2 package and simply finds more of what it already knew.
pub const FORMAT_VERSION: u32 = 1;

/// The package's default file extension.
pub const PACKAGE_EXTENSION: &str = "rdbak";

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = PACKAGE_MAGIC.len() + SALT_LEN + NONCE_LEN + 4;
const KEY_LEN: usize = 32;

/// PBKDF2 rounds. OWASP's 2023 floor for PBKDF2-HMAC-SHA256; the header carries the number, so a
/// future batch can raise it without breaking packages written under the old one.
const PBKDF2_ITERATIONS: u32 = 210_000;

/// The **data directory** root name (AV-1): `settings.json`, `sessions.db`, `token`, `node.key`,
/// `peers.json`, `rooms.json`.
pub const ROOT_DATA_DIR: &str = "data-dir";

/// The **workspace** root name (AV-2): its `.riscdom/` — the audit store and the snapshots (§1.2).
pub const ROOT_WORKSPACE: &str = "workspace";

/// The **keyring** root name (AV-2): the credentials derived from `settings.json` (§1.4).
pub const ROOT_KEYRING: &str = "keyring";

/// The audit store's path inside the workspace (§1.2).
pub const AUDIT_DB_IN_WORKSPACE: &str = ".riscdom/audit.db";

/// The snapshots' directory inside the workspace (§1.2).
pub const SNAPSHOTS_IN_WORKSPACE: &str = ".riscdom/snapshots";

/// The standing `not_derived` line: the OS keyring cannot be enumerated, so an entry whose owner is
/// gone from `settings.json` cannot be named. It is always present, because it is always true — and
/// [backup.md](../../docs/backup.md) §1.4 calls this list the package's **one declared outside
/// dependency**, so it is stated rather than hidden.
pub const NOT_DERIVED_UNNAMEABLE: &str = "unnameable: the OS keyring cannot be listed, so a credential whose executor or host no longer appears in settings.json cannot be named and is not in this package";

/// A `not_derived` line for an account that `settings.json` names but the keyring does not hold.
pub fn not_derived_missing(account: &str) -> String {
    format!("missing: {account} is named by settings.json but is not in the OS keyring")
}

/// A `not_derived` line for a keyring that would not answer.
pub fn not_derived_keyring_error(error: &str) -> String {
    format!("unreadable: the OS keyring could not be read ({error})")
}

/// The `not_derived` line for a `settings.json` this build could not read.
pub const NOT_DERIVED_SETTINGS_UNREADABLE: &str =
    "unreadable: settings.json could not be read, so no keyring account could be named";

/// The data directory's files that have no home in `host_core::paths`. Each name is the owner's
/// constant — `server::token::TOKEN_FILE`, `net::identity::NODE_KEY_FILE`, `net::peers::PEERS_FILE`,
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
    /// Which root the file came from: `data-dir`, `workspace` or `keyring` (§1).
    pub root: String,
    /// The path inside that root, e.g. `settings.json`, `.riscdom/audit.db`,
    /// `.riscdom/snapshots/local/x/state.mig`, or a keyring account name.
    pub path: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its SHA-256, lowercase hex.
    pub sha256: String,
    /// The format's marker, when the file has one: `version=2`, `schema_version=1`,
    /// `user_version=1`. `None` for a file that has no marker (the token file, a keyring entry).
    pub marker: Option<String>,
}

/// The root manifest: what the package holds, and the little that names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The tool that wrote it.
    pub tool: String,
    /// [`FORMAT_VERSION`].
    pub format: u32,
    /// The moment of export, epoch milliseconds.
    pub exported_at_ms: i64,
    /// The node's `node_id`, which is its device name ([`agent::identity::device`]).
    pub node_id: String,
    /// Every file the package carries.
    pub entries: Vec<ManifestEntry>,
    /// What the tool could **not** carry: the credentials §1.4 cannot name, plus the standing note
    /// that the OS keyring cannot be listed at all. See [`NOT_DERIVED_UNNAMEABLE`].
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
    /// The audit store could not be taken through SQLite's consistent path.
    #[error("the audit store could not be exported: {0}")]
    Audit(String),
    /// The OS random source refused.
    #[error("the OS random source failed")]
    Random,
    /// The cipher refused (an internal error, never a bad passphrase).
    #[error("the cipher refused to run")]
    Cipher,
    /// The passphrase was wrong, or the package was altered — the two are one answer on purpose.
    #[error("the passphrase is wrong, or the package was altered")]
    PassphraseOrTampered,
    /// The bytes are not a package this build can read.
    #[error("not a readable riscdom-backup package: {0}")]
    Malformed(String),
}

/// Where an entry's bytes come from.
enum Source {
    /// A file on disk, appended as it is.
    File(PathBuf),
    /// Bytes already in hand (the audit store's consistent copy, a keyring entry).
    Bytes(Vec<u8>),
}

/// One entry found for the package, with the manifest record it produced.
struct Collected {
    entry: ManifestEntry,
    source: Source,
}

impl Collected {
    /// The entry's name inside the archive: its root, then its path.
    fn archive_name(&self) -> String {
        let root = match self.entry.root.as_str() {
            ROOT_WORKSPACE => ROOT_WORKSPACE,
            ROOT_KEYRING => ROOT_KEYRING,
            _ => ROOT_DATA_DIR,
        };
        format!("{root}/{}", self.entry.path)
    }
}

/// Export a node as one sealed package, reading its credentials from the **OS keyring**.
///
/// `data_dir` is the directory a node keeps `settings.json`, `sessions.db`, `token`, `node.key`,
/// `peers.json` and `rooms.json` in; `workspace` is the directory it works in, whose `.riscdom/`
/// holds the audit store and the snapshots. A file or a directory that is absent is skipped, not an
/// error.
pub fn export(
    data_dir: impl AsRef<Path>,
    workspace: impl AsRef<Path>,
    phrase: &[u8],
) -> Result<Exported, BackupError> {
    export_with(
        data_dir,
        workspace,
        phrase,
        &host_core::keyring::OsKeyring::new(),
    )
}

/// The same export with an injected keyring, so a test never touches the machine's credential store.
pub fn export_with(
    data_dir: impl AsRef<Path>,
    workspace: impl AsRef<Path>,
    phrase: &[u8],
    keyring: &dyn KeyringBackend,
) -> Result<Exported, BackupError> {
    let data_dir = data_dir.as_ref();
    let workspace = workspace.as_ref();

    let mut entries = collect_data_dir(data_dir)?;
    if let Some(audit) = collect_audit_store(workspace)? {
        entries.push(audit);
    }
    entries.extend(collect_snapshots(workspace)?);
    let (credentials, mut not_derived) = collect_credentials(data_dir, keyring);
    entries.extend(credentials);
    not_derived.push(NOT_DERIVED_UNNAMEABLE.to_string());

    let manifest = Manifest {
        tool: "riscdom-backup".to_string(),
        format: FORMAT_VERSION,
        exported_at_ms: now_ms(),
        node_id: agent::identity::device(),
        entries: entries.iter().map(|file| file.entry.clone()).collect(),
        not_derived,
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest)?;
    let tar = build_tar(&entries, &manifest_json)?;
    let gzipped = gzip(&tar)?;
    let bytes = encrypt(&gzipped, phrase)?;
    Ok(Exported { bytes, manifest })
}

/// The data-directory files (§1.1), in the order the document lists them.
fn collect_data_dir(data_dir: &Path) -> Result<Vec<Collected>, BackupError> {
    let mut wanted: Vec<(String, PathBuf)> = Vec::new();
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
        if let Some(collected) = collect_file(ROOT_DATA_DIR, name, path)? {
            out.push(collected);
        }
    }
    Ok(out)
}

/// One file as an entry, or `None` when it is absent (absent is not an error).
fn collect_file(
    root: &str,
    path_in_root: String,
    path: PathBuf,
) -> Result<Option<Collected>, BackupError> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)?;
    let marker = marker_for(&path_in_root, &bytes);
    Ok(Some(Collected {
        entry: ManifestEntry {
            root: root.to_string(),
            path: path_in_root,
            size: bytes.len() as u64,
            sha256: hex(&Sha256::digest(&bytes)),
            marker,
        },
        source: Source::File(path),
    }))
}

/// The audit store, through **SQLite's own consistent path** (§2).
///
/// `audit.db` is **WAL** and opened by several processes at once, so a byte copy of the file alone
/// can miss frames still in `-wal`. `VACUUM INTO` asks SQLite for a fresh, consistent database, and
/// the copy that comes back is the one that goes in the package.
fn collect_audit_store(workspace: &Path) -> Result<Option<Collected>, BackupError> {
    let source = workspace.join(AUDIT_DB_IN_WORKSPACE);
    if !source.is_file() {
        return Ok(None);
    }
    let destination = TempPath::new("vacuum")?;
    let target = destination.as_str()?;

    let connection =
        rusqlite::Connection::open_with_flags(&source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| {
                BackupError::Audit(format!("the audit store could not be opened: {error}"))
            })?;
    connection
        .execute("VACUUM INTO ?1", [target.as_str()])
        .map_err(|error| BackupError::Audit(format!("VACUUM INTO refused: {error}")))?;
    drop(connection);

    let bytes = fs::read(destination.path())?;
    Ok(Some(Collected {
        entry: ManifestEntry {
            root: ROOT_WORKSPACE.to_string(),
            path: AUDIT_DB_IN_WORKSPACE.to_string(),
            size: bytes.len() as u64,
            sha256: hex(&Sha256::digest(&bytes)),
            marker: marker_for("audit.db", &bytes),
        },
        source: Source::Bytes(bytes),
    }))
}

/// Every snapshot under `<workspace>/.riscdom/snapshots/`, walked whole (§1.2). A missing directory
/// is not an error — a node that never snapshotted has none.
fn collect_snapshots(workspace: &Path) -> Result<Vec<Collected>, BackupError> {
    let root = workspace.join(SNAPSHOTS_IN_WORKSPACE);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    walk(&root, SNAPSHOTS_IN_WORKSPACE.to_string(), &mut found)?;
    found.sort_by(|left, right| left.0.cmp(&right.0));

    let mut out = Vec::new();
    for (path_in_root, path) in found {
        if let Some(collected) = collect_file(ROOT_WORKSPACE, path_in_root, path)? {
            out.push(collected);
        }
    }
    Ok(out)
}

/// A depth-first walk that keeps the tree's own structure; entries are sorted by their caller.
fn walk(root: &Path, prefix: String, out: &mut Vec<(String, PathBuf)>) -> Result<(), BackupError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let child = format!("{prefix}/{name}");
        if path.is_dir() {
            walk(&path, child, out)?;
        } else if path.is_file() {
            out.push((child, path));
        }
    }
    Ok(())
}

/// The credentials `settings.json` lets us **name** (§1.4).
///
/// The OS keyring cannot be listed, so this derives the account names it expects and reads exactly
/// those: `llm-api-key:<executor_id>:<provider_id>` and the legacy `llm-api-key:<provider_id>` from
/// `llm_configs`, and `remote-token:<host>` from `network.remote_url` (the front end files the token
/// under that string, trimmed). Every account the keyring does not hold — and every failure to read
/// it at all — becomes a line in `not_derived`, so a gap is reported rather than assumed away.
/// Returns the carried entries and those lines; the caller adds the standing unnameable note.
fn collect_credentials(
    data_dir: &Path,
    keyring: &dyn KeyringBackend,
) -> (Vec<Collected>, Vec<String>) {
    let mut carried = Vec::new();
    let mut not_derived = Vec::new();

    let settings_path = host_core::paths::settings_path_in(data_dir);
    if !settings_path.is_file() {
        // No settings file is a fresh node, not a gap: there is simply nothing to name.
        return (carried, not_derived);
    }
    let settings = match fs::read(&settings_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<host_core::settings::LocalSettings>(&bytes).ok())
    {
        Some(settings) => settings,
        None => {
            not_derived.push(NOT_DERIVED_SETTINGS_UNREADABLE.to_string());
            return (carried, not_derived);
        }
    };

    for account in derive_accounts(&settings) {
        match keyring.get(host_core::keyring::SERVICE, &account) {
            Ok(Some(secret)) => {
                let bytes = secret.into_bytes();
                carried.push(Collected {
                    entry: ManifestEntry {
                        root: ROOT_KEYRING.to_string(),
                        path: account,
                        size: bytes.len() as u64,
                        sha256: hex(&Sha256::digest(&bytes)),
                        marker: None,
                    },
                    source: Source::Bytes(bytes),
                });
            }
            Ok(None) => not_derived.push(not_derived_missing(&account)),
            Err(error) => not_derived.push(not_derived_keyring_error(&error)),
        }
    }
    (carried, not_derived)
}

/// The account names `settings.json` implies, in a stable order and without duplicates.
fn derive_accounts(settings: &host_core::settings::LocalSettings) -> Vec<String> {
    use host_core::keyring::{legacy_user_for_provider, user_for_llm_key, user_for_remote};

    let mut accounts: Vec<String> = Vec::new();
    let mut push = |account: String| {
        if !accounts.contains(&account) {
            accounts.push(account);
        }
    };

    let mut executor_ids: Vec<&String> = settings.llm_configs.keys().collect();
    executor_ids.sort();
    for executor_id in executor_ids {
        let entry = &settings.llm_configs[executor_id];
        if entry.provider_id.is_empty() {
            continue;
        }
        push(user_for_llm_key(executor_id, &entry.provider_id));
        push(legacy_user_for_provider(&entry.provider_id));
    }

    if let Some(network) = &settings.network {
        if let Some(remote) = &network.remote_url {
            let host = remote.trim();
            if !host.is_empty() {
                push(user_for_remote(host));
            }
        }
    }
    accounts
}

/// The format's marker, read the way [api-compatibility.md](../../docs/api-compatibility.md) §6
/// reads it — a version the file itself carries, never a guess.
fn marker_for(name: &str, bytes: &[u8]) -> Option<String> {
    if name == "settings.json" {
        return json_marker(bytes, "version");
    }
    if name == "node.key" || name == "peers.json" || name == "rooms.json" {
        return json_marker(bytes, "schema_version");
    }
    if name.ends_with(".db") {
        return sqlite_user_version(bytes).map(|v| format!("user_version={v}"));
    }
    // The control-plane token has no version at all: one line of hex, shape-checked (§1.1), and a
    // keyring entry is bytes with no format of its own.
    None
}

/// `key=<n>` from a JSON document's top-level member, or `None` when it is absent or unreadable.
fn json_marker(bytes: &[u8], key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let member = value.get(key)?;
    Some(format!("{key}={member}"))
}

/// SQLite keeps `user_version` as a big-endian `u32` at byte 60 of the file header — the format's
/// own layout, so reading it needs no query.
fn sqlite_user_version(bytes: &[u8]) -> Option<u32> {
    const OFFSET: usize = 60;
    let raw: [u8; 4] = bytes.get(OFFSET..OFFSET + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(raw))
}

/// `manifest.json` first, then every entry under its root — the layout §2 describes.
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
        let name = file.archive_name();
        match &file.source {
            Source::File(path) => {
                let mut source = fs::File::open(path)?;
                builder
                    .append_file(name, &mut source)
                    .map_err(archive_error)?;
            }
            Source::Bytes(bytes) => {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o600);
                header.set_mtime(0);
                header.set_cksum();
                builder
                    .append_data(&mut header, name, bytes.as_slice())
                    .map_err(archive_error)?;
            }
        }
    }
    builder.into_inner().map_err(BackupError::Io)
}

fn archive_error(error: std::io::Error) -> BackupError {
    BackupError::Archive(error.to_string())
}

/// gzip the tar. Compression comes **before** encryption: an encrypted archive cannot be compressed
/// afterwards, and compressing first leaks only the length, which is inside the seal anyway.
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
pub fn encrypt(plaintext: &[u8], phrase: &[u8]) -> Result<Vec<u8>, BackupError> {
    use ring::rand::SecureRandom;
    let rng = ring::rand::SystemRandom::new();
    let mut salt = [0u8; SALT_LEN];
    rng.fill(&mut salt).map_err(|_| BackupError::Random)?;
    let mut nonce = [0u8; NONCE_LEN];
    rng.fill(&mut nonce).map_err(|_| BackupError::Random)?;

    let key_bytes = derive_key(phrase, &salt, PBKDF2_ITERATIONS)?;
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
pub fn decrypt(package: &[u8], phrase: &[u8]) -> Result<Vec<u8>, BackupError> {
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
    let key_bytes = derive_key(phrase, salt, iterations)?;
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
pub fn read_manifest(package: &[u8], phrase: &[u8]) -> Result<Manifest, BackupError> {
    let tar = gunzip(&decrypt(package, phrase)?)?;
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
fn derive_key(phrase: &[u8], salt: &[u8], iterations: u32) -> Result<[u8; KEY_LEN], BackupError> {
    let rounds = NonZeroU32::new(iterations).ok_or(BackupError::Cipher)?;
    let mut key = [0u8; KEY_LEN];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        rounds,
        salt,
        phrase,
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

/// A path under the system temp directory that does not exist yet, removed on drop — where
/// `VACUUM INTO` writes its consistent copy.
struct TempPath {
    path: PathBuf,
}

impl TempPath {
    fn new(tag: &str) -> Result<Self, BackupError> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "riscdom-backup-{tag}-{}-{n}-{nanos}.db",
            std::process::id()
        ));
        // `VACUUM INTO` refuses a target that exists, so make sure of that first.
        let _ = fs::remove_file(&path);
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn as_str(&self) -> Result<String, BackupError> {
        self.path
            .to_str()
            .map(str::to_string)
            .ok_or_else(|| BackupError::Audit("the temporary path is not valid UTF-8".into()))
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
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

    /// A real SQLite database with one row, so `VACUUM INTO` has something to prove.
    fn real_audit_db(workspace: &Path) -> PathBuf {
        let dir = workspace.join(".riscdom");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("audit.db");
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA user_version = 1;
                 CREATE TABLE audit_events (id INTEGER PRIMARY KEY, body TEXT);
                 INSERT INTO audit_events (body) VALUES ('an event');",
            )
            .unwrap();
        drop(connection);
        path
    }

    fn export_of(dir: &Path, workspace: &Path, keyring: &dyn KeyringBackend) -> Exported {
        export_with(dir, workspace, b"test passphrase", keyring).unwrap()
    }

    /// The archive's entries, as (name, bytes).
    fn unpack(package: &[u8], phrase: &[u8]) -> Vec<(String, Vec<u8>)> {
        let tar = gunzip(&decrypt(package, phrase).unwrap()).unwrap();
        let mut archive = tar::Archive::new(tar.as_slice());
        let mut out = Vec::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().to_string();
            let mut body = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut body).unwrap();
            out.push((name, body));
        }
        out
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
    fn the_manifest_names_every_data_dir_file_and_its_marker() {
        let dir = temp_dir("manifest");
        let workspace = temp_dir("manifest-ws");
        populated(&dir);
        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());

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
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn the_payload_carries_every_file_byte_for_byte() {
        let dir = temp_dir("payload");
        let workspace = temp_dir("payload-ws");
        populated(&dir);
        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());

        let mut compared = 0;
        for (name, body) in unpack(&exported.bytes, b"test passphrase") {
            if name == "manifest.json" {
                let parsed: Manifest = serde_json::from_slice(&body).unwrap();
                assert_eq!(parsed, exported.manifest);
                continue;
            }
            let path = name.strip_prefix("data-dir/").unwrap();
            assert_eq!(body, fs::read(dir.join(path)).unwrap(), "{path} differs");
            compared += 1;
        }
        assert_eq!(compared, 6);

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn an_empty_node_exports_an_empty_manifest_with_the_unnameable_note() {
        let dir = temp_dir("empty");
        let workspace = temp_dir("empty-ws");
        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());
        assert!(exported.manifest.entries.is_empty());
        assert_eq!(exported.manifest.format, FORMAT_VERSION);
        assert!(!exported.manifest.node_id.is_empty());
        // The standing line is always there: the keyring cannot be listed (§1.4).
        assert_eq!(exported.manifest.not_derived.len(), 1);
        assert!(exported.manifest.not_derived[0].starts_with("unnameable:"));
        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn absent_files_are_skipped_not_failed() {
        let dir = temp_dir("partial");
        let workspace = temp_dir("partial-ws");
        fs::write(dir.join("settings.json"), br#"{"version":2}"#).unwrap();
        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());
        assert_eq!(exported.manifest.entries.len(), 1);
        assert_eq!(exported.manifest.entries[0].path, "settings.json");
        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn the_audit_store_is_taken_through_sqlite_and_opens() {
        let dir = temp_dir("audit");
        let workspace = temp_dir("audit-ws");
        let source = real_audit_db(&workspace);
        let before = fs::read(&source).unwrap();

        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());
        let entry = exported
            .manifest
            .entries
            .iter()
            .find(|entry| entry.root == ROOT_WORKSPACE && entry.path == AUDIT_DB_IN_WORKSPACE)
            .expect("the audit store must be in the manifest");
        assert_eq!(entry.marker.as_deref(), Some("user_version=1"));

        // The carried copy is a database, and its row survived the trip.
        let carried = unpack(&exported.bytes, b"test passphrase")
            .into_iter()
            .find(|(name, _)| name == &format!("{ROOT_WORKSPACE}/{AUDIT_DB_IN_WORKSPACE}"))
            .map(|(_, body)| body)
            .unwrap();
        let copy = temp_dir("audit-copy").join("audit.db");
        fs::write(&copy, &carried).unwrap();
        let connection = rusqlite::Connection::open_with_flags(
            &copy,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM audit_events", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        // And it really is a copy, not the original bytes: the source is WAL and has a live `-wal`.
        assert_ne!(carried, before);

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn a_missing_audit_store_is_not_an_error() {
        let dir = temp_dir("no-audit");
        let workspace = temp_dir("no-audit-ws");
        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());
        assert!(exported
            .manifest
            .entries
            .iter()
            .all(|entry| entry.root != ROOT_WORKSPACE));
        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn the_snapshot_tree_is_walked_whole() {
        let dir = temp_dir("snap");
        let workspace = temp_dir("snap-ws");
        let nested = workspace
            .join(SNAPSHOTS_IN_WORKSPACE)
            .join("local")
            .join("local-1-1");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("state.mig"), b"a snapshot").unwrap();
        fs::write(nested.join("meta.json"), b"{\"mode\":\"tcp-relay\"}").unwrap();
        let second = workspace
            .join(SNAPSHOTS_IN_WORKSPACE)
            .join("local")
            .join("local-1-2");
        fs::create_dir_all(&second).unwrap();
        fs::write(second.join("state.mig"), b"another").unwrap();

        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());
        let snapshots: Vec<&ManifestEntry> = exported
            .manifest
            .entries
            .iter()
            .filter(|entry| entry.root == ROOT_WORKSPACE && entry.path.contains("/snapshots/"))
            .collect();
        assert_eq!(snapshots.len(), 3);
        assert!(snapshots
            .iter()
            .any(|entry| { entry.path == ".riscdom/snapshots/local/local-1-1/state.mig" }));
        assert!(snapshots
            .iter()
            .any(|entry| { entry.path == ".riscdom/snapshots/local/local-1-2/state.mig" }));

        let names: Vec<String> = unpack(&exported.bytes, b"test passphrase")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(
            names.contains(&"workspace/.riscdom/snapshots/local/local-1-1/meta.json".to_string())
        );

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn a_missing_snapshots_directory_is_not_an_error() {
        let dir = temp_dir("no-snap");
        let workspace = temp_dir("no-snap-ws");
        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());
        assert!(exported
            .manifest
            .entries
            .iter()
            .all(|entry| !entry.path.contains("snapshots")));
        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn the_accounts_are_derived_from_settings() {
        let mut settings = host_core::settings::LocalSettings::default();
        settings.llm_configs.insert(
            "local".to_string(),
            host_core::settings::LlmConfigEntry {
                provider_id: "deepseek".to_string(),
                ..Default::default()
            },
        );
        settings.llm_configs.insert(
            "worker-a".to_string(),
            host_core::settings::LlmConfigEntry {
                provider_id: "deepseek".to_string(),
                ..Default::default()
            },
        );
        settings.llm_configs.insert(
            "empty".to_string(),
            host_core::settings::LlmConfigEntry::default(),
        );
        settings.network = Some(host_core::settings::NetworkSettings {
            remote_url: Some("  192.168.1.10:7821  ".to_string()),
            ..Default::default()
        });

        let accounts = derive_accounts(&settings);
        assert!(accounts.contains(&"llm-api-key:local:deepseek".to_string()));
        assert!(accounts.contains(&"llm-api-key:worker-a:deepseek".to_string()));
        assert!(accounts.contains(&"llm-api-key:deepseek".to_string()));
        assert!(accounts.contains(&"remote-token:192.168.1.10:7821".to_string()));
        // A config with no provider names nothing.
        assert!(!accounts.iter().any(|account| account.contains(":empty:")));
        // The legacy spelling appears once, not once per executor.
        assert_eq!(
            accounts
                .iter()
                .filter(|account| *account == "llm-api-key:deepseek")
                .count(),
            1
        );
    }

    #[test]
    fn credentials_that_exist_are_carried_and_those_that_do_not_are_reported() {
        let dir = temp_dir("keyring");
        let workspace = temp_dir("keyring-ws");
        fs::write(
            dir.join("settings.json"),
            br#"{"version":2,"llm_configs":{"local":{"provider_id":"deepseek"}},"network":{"remote_url":"192.168.1.10:7821"}}"#,
        )
        .unwrap();

        let keyring = InMemoryKeyring::new();
        keyring
            .set(
                host_core::keyring::SERVICE,
                "llm-api-key:local:deepseek",
                "sk-a-secret",
            )
            .unwrap();

        let exported = export_of(&dir, &workspace, &keyring);

        let carried: Vec<&ManifestEntry> = exported
            .manifest
            .entries
            .iter()
            .filter(|entry| entry.root == ROOT_KEYRING)
            .collect();
        assert_eq!(carried.len(), 1);
        assert_eq!(carried[0].path, "llm-api-key:local:deepseek");
        assert_eq!(carried[0].size, "sk-a-secret".len() as u64);

        // The secret's bytes are in the package, under the keyring root.
        let names: Vec<String> = unpack(&exported.bytes, b"test passphrase")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(names.contains(&format!("{ROOT_KEYRING}/llm-api-key:local:deepseek")));

        // The two it could not find, and the standing note, are reported.
        assert!(exported
            .manifest
            .not_derived
            .iter()
            .any(|line| line.contains("llm-api-key:deepseek is named by settings.json")));
        assert!(exported
            .manifest
            .not_derived
            .iter()
            .any(|line| line.contains("remote-token:192.168.1.10:7821 is named by settings.json")));
        assert!(exported
            .manifest
            .not_derived
            .iter()
            .any(|line| line.starts_with("unnameable:")));

        // Nothing in the manifest holds the secret itself.
        let manifest_json = serde_json::to_string(&exported.manifest).unwrap();
        assert!(!manifest_json.contains("sk-a-secret"));

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }

    #[test]
    fn unreadable_settings_report_the_gap_instead_of_guessing() {
        let dir = temp_dir("bad-settings");
        let workspace = temp_dir("bad-settings-ws");
        fs::write(dir.join("settings.json"), b"this is not JSON").unwrap();
        let exported = export_of(&dir, &workspace, &InMemoryKeyring::new());
        assert!(exported
            .manifest
            .not_derived
            .iter()
            .any(|line| line.contains("settings.json could not be read")));
        assert!(exported
            .manifest
            .entries
            .iter()
            .all(|e| e.root != ROOT_KEYRING));
        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&workspace).ok();
    }
}
