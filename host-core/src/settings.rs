//! Local, non-secret settings (`settings.json`).
//!
//! Only non-secret preferences live here — **never** an API key (those go to
//! the OS keyring). Missing, unreadable or corrupt files degrade to defaults;
//! loading never fails and never panics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Current on-disk schema version.
///
/// **v2** (v1.0 M2b-1): the LLM configuration became a per-executor map
/// ([`LocalSettings::llm_configs`]). v1 files are migrated on open — the first
/// real use of the mechanism `docs/api-compatibility.md` §6 describes — and a file
/// from a **newer** build is refused rather than half-read.
pub const SETTINGS_VERSION: u32 = 2;

/// The version a v1 file is treated as carrying when it declares none.
const OLDEST_VERSION: u32 = 1;

/// Labels an executor may **not** carry, because the node already uses them
/// (v1.0 M2b-3a).
///
/// An executor is addressed in the same key space as the node itself: the node's
/// own executor id is its device name ([`agent::DEFAULT_DEVICE`], `"local"`, until
/// a node names itself), and `"*"` is the wildcard a session query spells as "every
/// executor". A worker labelled either one would be a second thing answering to a
/// name that already means something — the LLM configuration is looked up by this
/// string, so a worker called `local` would share the node's own model entry.
///
/// The list is a **refusal list, not a file error**: a settings file carrying one
/// loads intact and the offender is skipped with a visible audit event, the same
/// way a newer file's refusal is visible (v1.0 M2b-1) instead of being swallowed.
/// Bricking a hand-edited file would be the worse answer.
pub const RESERVED_EXECUTOR_LABELS: &[&str] = &[agent::DEFAULT_DEVICE, "*"];

/// Is `label` one of [`RESERVED_EXECUTOR_LABELS`]?
///
/// Compared against the node's **current** device name as well as the list, so a
/// node that has named itself keeps its own name reserved too.
pub fn is_reserved_executor_label(label: &str) -> bool {
    RESERVED_EXECUTOR_LABELS.contains(&label) || label == agent::device()
}

/// One executor's model configuration, as **persisted** (v1.0 M2b-1).
///
/// Deliberately without an `api_key`: the key is a credential and lives in the OS
/// keyring, which is the rule `settings.json` has followed since v0.4. What is
/// here is the non-secret part a restart needs — which provider, which endpoint,
/// which model — so a node knows what it is configured for before anybody types a
/// key again.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LlmConfigEntry {
    #[serde(default)]
    pub provider_id: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
}

/// What reading a settings document did (v1.0 M2b-1).
///
/// The caller acts on this instead of guessing: a migrated file is written back
/// (after a backup), a newer one is reported and **nothing** is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsLoad {
    /// No file, or a file this build cannot parse at all: defaults, nothing to do.
    Missing,
    /// A file whose version is the one this build writes.
    Current,
    /// An older file, migrated in memory; the caller writes it back and keeps a
    /// backup of what was there.
    Migrated { from: u32 },
    /// A file from a **newer** build: refused, and nothing was applied.
    TooNew { found: u32 },
}

/// Why a settings document could not be read (v1.0 M2b-1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsLoadError {
    /// The file was written by a newer build.
    ///
    /// `docs/api-compatibility.md` §6: an old reader never reads a newer format,
    /// never reads part of it, and never downgrades it silently. The host reports
    /// this instead of carrying on with defaults as if nothing were wrong.
    #[error("data_too_new: settings.json is version {found}, this build reads {supported}")]
    DataTooNew { found: u32, supported: u32 },
}

/// Persisted local settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSettings {
    /// Schema version (normalised to [`SETTINGS_VERSION`] on load).
    pub version: u32,
    /// Manual RISC-V GCC path; `None` means auto-discovery.
    #[serde(default)]
    pub toolchain_path: Option<String>,
    /// Manual Zig executable path (v0.9 F3a); `None` means auto-discovery.
    ///
    /// A second, independent single value rather than a map: the language is chosen by
    /// the source extension, so `toolchain_path` and `zig_path` name **two** compilers
    /// for the same sandbox instead of two entries for one. Additive, exactly like
    /// `sandboxes`: a file written before this field existed loads with `None`, and
    /// `SETTINGS_VERSION` does not move.
    #[serde(default)]
    pub zig_path: Option<String>,
    /// Manual Rust sysroot (v0.9 F3b-1); `None` means the environment (`RISCDOM_RUST_SYSROOT`).
    ///
    /// The third single value beside `toolchain_path` and `zig_path`. It names a
    /// **directory**, not an executable: the `rust-std-<target>/` tree that carries `core`.
    /// Additive like the others: an older file loads with `None` and `SETTINGS_VERSION` does
    /// not move.
    #[serde(default)]
    pub rust_sysroot: Option<String>,
    /// Manual QEMU executable path; `None` means auto-discovery (v0.3 5b-1a).
    #[serde(default)]
    pub qemu_path: Option<String>,
    /// Last environment capability preflight, bound to the configuration
    /// fingerprint it was produced for (v0.4 batch 3).
    #[serde(default)]
    pub preflight: Option<crate::preflight::PreflightCache>,
    /// UI theme preference: `light`, `dark` or `system` (v0.4 #11a).
    #[serde(default)]
    pub theme: Option<String>,
    /// UI language preference: `system`, `en` or `zh` (v0.7 batch 2).
    #[serde(default)]
    pub language: Option<String>,
    /// Alert (banner + popup) when an audit write fails (v0.8).
    ///
    /// Defaults to `true`, including for a settings file written before this
    /// field existed: the log line and the `audit:failed` event are always sent,
    /// this only controls whether the interface shouts about it.
    #[serde(default = "default_alert_on_audit_failure")]
    pub alert_on_audit_failure: bool,
    /// Sandbox definitions written by hand (v0.9 sandbox F2a).
    ///
    /// Additive: a file written before this field existed loads with an empty
    /// list, and a file that carries it is still readable by a version that does
    /// not know it (`SETTINGS_VERSION` does not move, F2a decision 1). The scan's
    /// own findings never land here — the registry is merged on read.
    #[serde(default)]
    pub sandboxes: Vec<crate::sandbox_def::SandboxDef>,
    /// Which definition a caller gets when it names none (v0.9 sandbox F2a).
    ///
    /// `None` means the built-in fallback
    /// ([`DEFAULT_SANDBOX_NAME`](crate::sandbox_def::DEFAULT_SANDBOX_NAME)).
    #[serde(default)]
    pub default_sandbox: Option<String>,
    /// The executor processes this node dispatches to (v0.9 interface E0).
    ///
    /// Additive, exactly like `sandboxes`: a file written before this field
    /// existed loads with an empty list and `SETTINGS_VERSION` does not move.
    /// An empty list is a working configuration — `POST /v0/tasks` then answers
    /// `404` for every target, because this node owns no executor. The node
    /// **itself** is not registered here: a caller that wants to run on this node
    /// uses `POST /v0/agent/run`.
    #[serde(default)]
    pub executors: Vec<ExecutorSpecSettings>,
    /// How this node talks to the network (v0.9.9 内网接入).
    ///
    /// Additive like every field above: a file written before it existed loads
    /// with `None` and `SETTINGS_VERSION` does not move. `None` means "no
    /// network wiring has been configured", which is exactly the behaviour of
    /// every version before this one: the desktop runs its embedded node and
    /// serves nobody.
    #[serde(default)]
    pub network: Option<NetworkSettings>,
    /// Every executor's model configuration, keyed by **executor id** (v1.0 M2b-1,
    /// `SETTINGS_VERSION` 2).
    ///
    /// The key is the node's own device name (`"local"` until a node names
    /// itself) for the machine itself, and `settings.executors[].label` for a
    /// worker — the same addressing space `Task.target` uses. Deliberately **not**
    /// the node's `AgentId`: that carries the pid, so a key made of it would not
    /// survive the restart it exists for.
    ///
    /// The values are non-secret ([`LlmConfigEntry`]); the keys live in the OS
    /// keyring. A v1 file has no such field at all and migrates to an **empty**
    /// map — nothing is guessed from the keyring or the environment.
    #[serde(default)]
    pub llm_configs: HashMap<String, LlmConfigEntry>,
}

/// The node's network wiring (v0.9.9).
///
/// Two directions, one struct, because they are configured on one screen:
/// **out** — this desktop connects to an in-network RiscDom server; **in** —
/// this desktop serves its own board to the network. Nothing here starts
/// anything by itself: the settings decide, and the wiring acts on them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NetworkSettings {
    /// The in-network server to connect **to** (`"out"`); `None` keeps the
    /// embedded host, which is what every version before this one did.
    ///
    /// **The token is not here.** A remote server's bearer token is a
    /// credential, and this file's own rule is that no secret lives in it: it
    /// goes to the OS keyring under `remote-token:<host>` (v0.9.9 `"out"`), so a
    /// settings file can be read, copied or backed up without leaking access to
    /// another machine.
    #[serde(default)]
    pub remote_url: Option<String>,
    /// Serve this node's board to the network (`"in"`).
    #[serde(default)]
    pub lan_enabled: bool,
    /// Where the embedded server binds. `None` means loopback
    /// (`127.0.0.1:7821`, the same default `riscdom-server` uses).
    #[serde(default)]
    pub lan_bind: Option<String>,
    /// Bind on every interface rather than loopback.
    ///
    /// The switch that makes the board reachable from a phone — and therefore
    /// the switch the interface has to warn about, since anyone on the network
    /// can then reach the node and only the token stands in the way.
    #[serde(default)]
    pub lan_allow_lan: bool,
    /// The peer that is this node's **cross-region server** (v1.0 V-2), or `None` for a node with no
    /// wide-area lane ([connection.md §6.4](../docs/connection.md): a node's network settings name
    /// which peer is its server).
    ///
    /// The value is a **`node_id`** that must appear in this node's own `peers.json`, because §6.4
    /// makes the server a peer and puts its public key there — which is what lets this node verify
    /// what the server signs. Additive, exactly like every field above: a file written before it
    /// existed loads with `None`, and `SETTINGS_VERSION` does not move.
    #[serde(default)]
    pub cross_region_server: Option<String>,
    /// Serve this node's **workgroup** as its in-network server (v1.0 AC-4), or `None` for a node
    /// that is only a client.
    ///
    /// The same role the standalone `riscdom-relay` binary runs
    /// ([connection.md §6.1](../docs/connection.md)), embedded in a node instead of standing alone:
    /// one mechanism, two deployment shapes. **A deployer configures it** — the project never
    /// starts a server and there is no default address. Additive like every field above.
    #[serde(default)]
    pub server_role: Option<ServerRoleSettings>,
}

/// This node's **server role**: it serves its workgroup as the network's server (v1.0 AC-4).
///
/// [connection.md §6.5](../docs/connection.md) is why this is a **deployment shape** and not a
/// new program: the in-network server is a node that also serves, running the same `RelayServer`
/// the dedicated `riscdom-relay` deployment runs. What that server knows is its own `peers.json`
/// and `rooms.json`, and it signs with its own `node.key` — all of them this node's files, which
/// is exactly why the role can be turned on here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerRoleSettings {
    /// Where the server role listens, e.g. `"0.0.0.0:7443"`.
    ///
    /// **Required, and deliberately with no default**: a default would be the project naming
    /// where a server is, which is what §6.1 forbids. The deployer writes the address.
    pub bind: String,
}

/// One executor the node can dispatch a task to (v0.9 interface E0).
///
/// This is the **settings** shape, not `worker::ExecutorSpec`: the worker depends
/// on this crate, so the dependency could not run the other way. It carries the
/// three things a command line is made of and nothing else.
///
/// **No `env`.** A settings file is not a secret store, and an environment block
/// is where a key would end up; the handle's `env` builders stay available to code
/// that is entitled to decide an executor's environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutorSpecSettings {
    /// The identity tasks address in `Task.target`. The child mints its **own**
    /// identity and reports it in the outcome; the label is the address, not the
    /// answer.
    pub label: String,
    /// The program to run (a `worker` binary, or anything that speaks the
    /// protocol: one task line in, one outcome line out, events on stderr).
    pub program: String,
    /// The program's arguments.
    #[serde(default)]
    pub args: Vec<String>,
}

/// The alert is on unless the user turns it off (v0.8).
fn default_alert_on_audit_failure() -> bool {
    true
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            toolchain_path: None,
            zig_path: None,
            rust_sysroot: None,
            qemu_path: None,
            preflight: None,
            theme: None,
            language: None,
            alert_on_audit_failure: true,
            sandboxes: Vec::new(),
            default_sandbox: None,
            executors: Vec::new(),
            network: None,
            llm_configs: HashMap::new(),
        }
    }
}

impl LocalSettings {
    /// Load from `path`, migrating an older file (v1.0 M2b-1).
    ///
    /// A missing, unreadable or malformed file yields [`LocalSettings::default`]
    /// instead of an error — the behaviour every version before this one had. A
    /// file from a **newer** build also yields defaults *here*, because this
    /// convenience cannot report the refusal; the host calls [`Self::load_text`]
    /// itself, which is the call that owns that decision.
    pub fn load(path: &Path) -> Self {
        let raw = std::fs::read_to_string(path).unwrap_or_default();
        Self::load_text(&raw).0
    }

    /// The settings in `raw`, plus what reading them did.
    ///
    /// Pure: no filesystem, no clock, no keyring — so the rules of
    /// `docs/api-compatibility.md` §6 can be pinned in a test. A document with no
    /// `version` key is the oldest format; one whose `version` is not a number is
    /// not a settings document at all (defaults, nothing applied); one from a
    /// newer build is **refused** rather than half-read.
    pub fn load_text(raw: &str) -> (Self, SettingsLoad) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
            return (Self::default(), SettingsLoad::Missing);
        };
        // A file written before the key existed is a v1 file; a version that is not
        // a number is not a version at all.
        let from = match value.get("version") {
            None => OLDEST_VERSION,
            Some(v) => match v.as_u64() {
                Some(n) => n as u32,
                None => return (Self::default(), SettingsLoad::Missing),
            },
        };
        match Self::migrate(value, from) {
            Ok(settings) if from < SETTINGS_VERSION => (settings, SettingsLoad::Migrated { from }),
            Ok(settings) => (settings, SettingsLoad::Current),
            Err(SettingsLoadError::DataTooNew { found, .. }) => {
                (Self::default(), SettingsLoad::TooNew { found })
            }
        }
    }

    /// Read `value` as the format `from`, migrating it to the current one.
    ///
    /// The single place a settings format changes. A step is a **structural**
    /// change, never a guess: v1 → v2 adds the per-executor LLM map empty, because
    /// a v1 file carried no LLM configuration at all (its key lives in the keyring
    /// and the rest lived only in memory) — there is nothing to invent.
    pub fn migrate(value: serde_json::Value, from: u32) -> Result<Self, SettingsLoadError> {
        if from > SETTINGS_VERSION {
            return Err(SettingsLoadError::DataTooNew {
                found: from,
                supported: SETTINGS_VERSION,
            });
        }
        let mut value = value;
        if from < 2 {
            value["llm_configs"] = serde_json::json!({});
        }
        value["version"] = serde_json::json!(SETTINGS_VERSION);

        let Ok(mut settings) = serde_json::from_value::<LocalSettings>(value) else {
            // A document whose *shape* is wrong is as harmless as unreadable JSON:
            // it was never a settings file this build understood.
            return Ok(Self::default());
        };
        // The load path's own tidying, unchanged: an empty path is no path, and an
        // executor needs both halves of a command line to be routable.
        settings.toolchain_path = settings.toolchain_path.filter(|p| !p.trim().is_empty());
        settings.zig_path = settings.zig_path.filter(|p| !p.trim().is_empty());
        settings.rust_sysroot = settings.rust_sysroot.filter(|p| !p.trim().is_empty());
        settings.qemu_path = settings.qemu_path.filter(|p| !p.trim().is_empty());
        settings
            .executors
            .retain(|e| !e.label.trim().is_empty() && !e.program.trim().is_empty());
        Ok(settings)
    }

    /// Persist to `path`, creating the parent directory when needed.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())
    }
}
