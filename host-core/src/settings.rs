//! Local, non-secret settings (`settings.json`).
//!
//! Only non-secret preferences live here — **never** an API key (those go to
//! the OS keyring). Missing, unreadable or corrupt files degrade to defaults;
//! loading never fails and never panics.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Current on-disk schema version.
pub const SETTINGS_VERSION: u32 = 1;

/// Persisted local settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSettings {
    /// Schema version (normalised to [`SETTINGS_VERSION`] on load).
    pub version: u32,
    /// Manual RISC-V GCC path; `None` means auto-discovery.
    #[serde(default)]
    pub toolchain_path: Option<String>,
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
            qemu_path: None,
            preflight: None,
            theme: None,
            language: None,
            alert_on_audit_failure: true,
            sandboxes: Vec::new(),
            default_sandbox: None,
            executors: Vec::new(),
        }
    }
}

impl LocalSettings {
    /// Load from `path`. A missing, unreadable or malformed file yields
    /// [`LocalSettings::default`] instead of an error.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<LocalSettings>(&text) {
            Ok(mut settings) => {
                settings.version = SETTINGS_VERSION;
                settings.toolchain_path = settings.toolchain_path.filter(|p| !p.trim().is_empty());
                settings.qemu_path = settings.qemu_path.filter(|p| !p.trim().is_empty());
                // An executor needs both halves of a command line to be routable:
                // a label with nothing to run is not an executor, and keeping it
                // would make `GET /v0/executors` promise something unreachable.
                settings
                    .executors
                    .retain(|e| !e.label.trim().is_empty() && !e.program.trim().is_empty());
                settings
            }
            Err(_) => Self::default(),
        }
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
