//! A versioned JSON file: `schema_version` first, migrate it or refuse it (v1.0 M4a).
//!
//! [decisions §11](../../docs/decisions.md) requires **every persisted format to carry
//! `schema_version` as its first field**, and [connection.md §2](../../docs/connection.md)
//! applies that to the connection layer's three files (`node.key`, `peers.json`,
//! `rooms.json`). This module is the one place that rule is implemented, so the three
//! do not each grow their own reading of it.
//!
//! **The shape follows [`host_core`'s `LocalSettings`], one level down**: four outcomes
//! for a read, and a version newer than this build is **refused** rather than
//! half-read. `SettingsLoad` is the model — `Missing` / `Current` / `Migrated` /
//! `TooNew` — because it is the one the project has already exercised with a real
//! migration (v1 → v2, v1.0 M2b-1).
//!
//! "First field" is a **write** rule and is enforced where it can be: the value is a
//! struct whose first member is `schema_version`, and `serde` writes struct members in
//! declaration order, so the bytes start with it. A reader cannot check the order —
//! `serde_json::Value`'s maps are unordered — which is why the test asserts the written
//! prefix instead of the parsed shape.

use serde::Serialize;
use std::io::Write;
use std::path::Path;

/// The lowest `schema_version` any format in this crate starts at.
pub const FIRST_SCHEMA_VERSION: u32 = 1;

/// What reading a versioned file did.
///
/// `Missing` and `TooNew` carry no value on purpose: the first is "there is nothing
/// here yet", and the second is "there is something here this build must not read".
/// A caller decides what to do with each — mint a file, or stay out of the way —
/// rather than being handed a half-understood value.
#[derive(Debug, Clone, PartialEq)]
pub enum VersionedLoad<T> {
    /// No file. Nothing was read and nothing was written.
    Missing,
    /// A file at the version this build writes.
    Current(T),
    /// An older file, read through its migration. The caller writes it back.
    Migrated { from: u32, value: T },
    /// A file from a **newer** build: refused, and nothing was applied.
    TooNew { found: u32 },
}

/// Why a versioned file could not be read or written.
#[derive(Debug)]
pub enum VersionedError {
    /// The filesystem refused.
    Io(std::io::Error),
    /// The bytes are not JSON, or not the shape this format claims.
    Json(String),
    /// The document is not a JSON object.
    NotAnObject,
    /// No `schema_version` member, or one that is not a non-negative integer.
    NoVersion,
    /// Older than this build can still read.
    TooOld { found: u32, oldest: u32 },
    /// The document parsed, but a member is missing or wrong (a bad `kty`, a key that
    /// is not 32 bytes, …). The format's own check, reported not repaired.
    Shape(String),
}

impl std::fmt::Display for VersionedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionedError::Io(e) => write!(f, "{e}"),
            VersionedError::Json(e) => write!(f, "the file is not JSON: {e}"),
            VersionedError::NotAnObject => write!(f, "the file must hold a JSON object"),
            VersionedError::NoVersion => write!(
                f,
                "the file carries no `schema_version`; it is not a versioned document"
            ),
            VersionedError::TooOld { found, oldest } => write!(
                f,
                "the file is version {found}; this build reads {oldest} and later"
            ),
            VersionedError::Shape(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for VersionedError {}

impl From<std::io::Error> for VersionedError {
    fn from(error: std::io::Error) -> Self {
        VersionedError::Io(error)
    }
}

/// A persisted format that carries `schema_version`.
pub trait Versioned: Sized {
    /// The version this build writes.
    const SCHEMA_VERSION: u32;
    /// The oldest version this build can still read. Defaults to the current one:
    /// a format with no migration yet can read exactly the version it writes.
    const OLDEST_SCHEMA_VERSION: u32 = Self::SCHEMA_VERSION;

    /// Read `value` as the format `from`.
    ///
    /// `from` is at most [`Self::SCHEMA_VERSION`], so a migration here only ever walks
    /// **forward**, and a format with no migration yet ignores it.
    fn from_value(value: serde_json::Value, from: u32) -> Result<Self, VersionedError>;
}

/// Read `path`, or answer that there is nothing there.
pub fn load<T: Versioned>(path: &Path) -> Result<VersionedLoad<T>, VersionedError> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(VersionedLoad::Missing)
        }
        Err(error) => return Err(VersionedError::Io(error)),
    };
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| VersionedError::Json(e.to_string()))?;
    let found = schema_version(&value)?;
    if found > T::SCHEMA_VERSION {
        return Ok(VersionedLoad::TooNew { found });
    }
    if found < T::OLDEST_SCHEMA_VERSION {
        return Err(VersionedError::TooOld {
            found,
            oldest: T::OLDEST_SCHEMA_VERSION,
        });
    }
    let parsed = T::from_value(value, found)?;
    Ok(if found < T::SCHEMA_VERSION {
        VersionedLoad::Migrated {
            from: found,
            value: parsed,
        }
    } else {
        VersionedLoad::Current(parsed)
    })
}

/// The `schema_version` a document carries.
fn schema_version(value: &serde_json::Value) -> Result<u32, VersionedError> {
    let object = value.as_object().ok_or(VersionedError::NotAnObject)?;
    object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(VersionedError::NoVersion)
}

/// Write `value` to `path`, replacing whatever is there.
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), VersionedError> {
    std::fs::write(path, to_bytes(value)?).map_err(VersionedError::Io)
}

/// Write `value` to `path` as a file only its owner can read.
///
/// `create_new` on purpose: this is for a **secret minted once** (a node's key), and
/// two processes racing to mint one must not both succeed. The mode is set at
/// creation on Unix so the umask cannot loosen it, and on Windows the ACL is edited
/// and then **read back** — a file that cannot be restricted is refused rather than
/// left readable by other accounts. The same rule, and the same technique, as the
/// bearer token (`server/src/token.rs`).
pub fn save_new_private<T: Serialize>(path: &Path, value: &T) -> Result<(), VersionedError> {
    let bytes = to_bytes(value)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    #[cfg(windows)]
    restrict_to_owner(path)?;
    Ok(())
}

/// The bytes a value is written as: pretty JSON, one trailing newline.
///
/// Pretty rather than compact because these are files a person may open, and the
/// trailing newline because a text file that does not end in one is a papercut in
/// every diff that follows.
fn to_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, VersionedError> {
    let mut text =
        serde_json::to_string_pretty(value).map_err(|e| VersionedError::Json(e.to_string()))?;
    text.push('\n');
    Ok(text.into_bytes())
}

/// Replace the file's ACL with "the current account, full control".
///
/// Windows has no `chmod`, so the ACL is edited with `icacls` and then read back: the
/// file must end up with exactly one principal, the current account.
#[cfg(windows)]
fn restrict_to_owner(path: &Path) -> Result<(), VersionedError> {
    let owner = current_principal()?;
    let status = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{owner}:F"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| VersionedError::Shape(format!("could not run icacls: {e}")))?;
    if !status.success() {
        return Err(VersionedError::Shape(
            "icacls could not set the owner-only ACL".into(),
        ));
    }
    let listing = std::process::Command::new("icacls")
        .arg(path)
        .output()
        .map_err(|e| VersionedError::Shape(format!("could not read the ACL back: {e}")))?;
    let text = String::from_utf8_lossy(&listing.stdout);
    let principals = acl_principals(&text);
    if principals.len() != 1 || principals[0] != owner {
        return Err(VersionedError::Shape(format!(
            "the ACL still lists {principals:?}"
        )));
    }
    Ok(())
}

/// The principals in an `icacls <file>` listing: the tokens that carry an
/// `(…F…)`-style rights group.
#[cfg(windows)]
fn acl_principals(listing: &str) -> Vec<String> {
    let mut principals = Vec::new();
    for token in listing.split_whitespace() {
        if let Some((principal, _)) = token.split_once(":(") {
            let principal = principal.trim_start_matches(|c: char| c.is_whitespace());
            if !principal.is_empty() {
                principals.push(principal.to_string());
            }
        }
    }
    principals
}

/// `DOMAIN\user` of the account running this process.
#[cfg(windows)]
fn current_principal() -> Result<String, VersionedError> {
    if let (Ok(domain), Ok(user)) = (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        if !domain.trim().is_empty() && !user.trim().is_empty() {
            return Ok(format!("{}\\{}", domain.trim(), user.trim()));
        }
    }
    let output = std::process::Command::new("whoami")
        .output()
        .map_err(|e| VersionedError::Shape(format!("could not run whoami: {e}")))?;
    let who = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if who.is_empty() {
        return Err(VersionedError::Shape("whoami printed nothing".into()));
    }
    Ok(who)
}

/// A versioned document used by this crate's own tests: two members, `schema_version`
/// first, and a migration from the version before it — so the loader's four outcomes
/// are exercised by something rather than asserted about in the abstract.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
pub(crate) struct Scratch {
    pub schema_version: u32,
    pub name: String,
    /// Added in version 2: an older file loads with the default.
    #[serde(default)]
    pub label: Option<String>,
}

#[cfg(test)]
impl Versioned for Scratch {
    const SCHEMA_VERSION: u32 = 2;
    const OLDEST_SCHEMA_VERSION: u32 = 1;

    fn from_value(mut value: serde_json::Value, from: u32) -> Result<Self, VersionedError> {
        if from == 1 {
            value["schema_version"] = serde_json::json!(Self::SCHEMA_VERSION);
        }
        serde_json::from_value(value).map_err(|e| VersionedError::Shape(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch_dir(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "riscdom-net-versioned-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_missing_file_is_missing_and_nothing_is_created() {
        let dir = scratch_dir("missing");
        let path = dir.join("nothing.json");
        let read = load::<Scratch>(&path).expect("load");
        assert_eq!(read, VersionedLoad::Missing);
        assert!(!path.exists(), "a read never creates the file");
    }

    #[test]
    fn the_written_bytes_start_with_the_version_member() {
        let dir = scratch_dir("first-field");
        let path = dir.join("scratch.json");
        save(
            &path,
            &Scratch {
                schema_version: Scratch::SCHEMA_VERSION,
                name: "one".into(),
                label: None,
            },
        )
        .expect("save");
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(
            raw.starts_with("{\n  \"schema_version\": 2,"),
            "schema_version must be the first field: {raw}"
        );
        assert!(raw.ends_with("}\n"), "one trailing newline: {raw:?}");
    }

    #[test]
    fn a_current_file_reads_back_as_it_was_written() {
        let dir = scratch_dir("current");
        let path = dir.join("scratch.json");
        let wanted = Scratch {
            schema_version: Scratch::SCHEMA_VERSION,
            name: "two".into(),
            label: Some("label".into()),
        };
        save(&path, &wanted).expect("save");
        assert_eq!(
            load::<Scratch>(&path).expect("load"),
            VersionedLoad::Current(wanted)
        );
    }

    #[test]
    fn an_older_file_is_migrated_and_a_newer_one_is_refused() {
        let dir = scratch_dir("versions");
        let older = dir.join("older.json");
        std::fs::write(&older, r#"{"schema_version": 1, "name": "old"}"#).expect("write");
        match load::<Scratch>(&older).expect("load") {
            VersionedLoad::Migrated { from, value } => {
                assert_eq!(from, 1);
                assert_eq!(value.name, "old");
                assert_eq!(value.schema_version, 2, "the migration stamps the new one");
                assert_eq!(value.label, None, "an absent member stays absent");
            }
            other => panic!("expected a migration, got {other:?}"),
        }

        let newer = dir.join("newer.json");
        std::fs::write(&newer, r#"{"schema_version": 3, "name": "new"}"#).expect("write");
        assert_eq!(
            load::<Scratch>(&newer).expect("load"),
            VersionedLoad::TooNew { found: 3 },
            "a newer file is refused, not half-read"
        );
    }

    #[test]
    fn a_document_without_a_version_is_not_a_versioned_file() {
        let dir = scratch_dir("no-version");
        let path = dir.join("scratch.json");
        std::fs::write(&path, r#"{"name": "nobody"}"#).expect("write");
        assert!(matches!(
            load::<Scratch>(&path).expect_err("no version"),
            VersionedError::NoVersion
        ));
        std::fs::write(&path, "[1, 2, 3]").expect("write");
        assert!(matches!(
            load::<Scratch>(&path).expect_err("not an object"),
            VersionedError::NotAnObject
        ));
        std::fs::write(&path, "not json at all").expect("write");
        assert!(matches!(
            load::<Scratch>(&path).expect_err("not json"),
            VersionedError::Json(_)
        ));
    }

    #[test]
    fn a_private_file_is_created_once_and_only_its_owner_can_read_it() {
        let dir = scratch_dir("private");
        let path = dir.join("secret.json");
        let value = Scratch {
            schema_version: Scratch::SCHEMA_VERSION,
            name: "secret".into(),
            label: None,
        };
        save_new_private(&path, &value).expect("first write");
        assert!(matches!(
            save_new_private(&path, &value),
            Err(VersionedError::Io(_))
        ));

        let metadata = std::fs::metadata(&path).expect("metadata");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        }
        let _ = metadata;
    }
}
