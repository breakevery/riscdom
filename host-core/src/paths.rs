//! Filesystem paths used by the host.
//!
//! Session data lives in the OS app-data directory -- never inside the repo and
//! never inside the AI workspace.
//!
//! Two ways to resolve a path:
//!
//! - the **injected** form (`*_in(base)`), where the caller passes the data
//!   directory it owns. This is what an [`AppState`](crate::AppState) built with
//!   [`AppState::with_data_dir`](crate::AppState::with_data_dir) uses, and it is
//!   what lets several agents in one process each keep their own data (v0.8);
//! - the **process-wide default** (`settings_path()` etc.), resolved from an
//!   environment override (tests), then the registered app-data directory, then
//!   a `<temp>/riscdom` fallback.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// The process-wide **default** app-data directory.
///
/// Only the default: an `AppState` built with
/// [`AppState::with_data_dir`](crate::AppState::with_data_dir) carries its own
/// directory and never consults this one. It was a `OnceLock` before v0.8, so
/// the first caller won and every later caller was silently ignored — the
/// dead-end the multi-agent work needed removed.
static APP_DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Register the default application data directory (called from the Tauri setup).
///
/// The **last** caller wins (v0.8; before that, later calls were ignored).
pub fn set_app_data_dir(dir: PathBuf) {
    if let Ok(mut slot) = APP_DATA_DIR.write() {
        *slot = Some(dir);
    }
}

/// The default application data directory, if registered.
pub fn app_data_dir() -> Option<PathBuf> {
    APP_DATA_DIR.read().ok().and_then(|slot| slot.clone())
}

/// The directory the process-wide defaults fall back to when nothing is set.
pub fn default_data_dir() -> PathBuf {
    app_data_dir().unwrap_or_else(|| std::env::temp_dir().join("riscdom"))
}

/// Environment override, else `fallback(base)`.
fn env_or_default(key: &str, fallback: impl FnOnce(&Path) -> PathBuf) -> PathBuf {
    if let Ok(path) = std::env::var(key) {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    fallback(&default_data_dir())
}

/// Path of the settings file **inside** `base`.
pub fn settings_path_in(base: &Path) -> PathBuf {
    base.join("settings.json")
}

/// Directory for downloaded toolchains **inside** `base` (created if missing).
pub fn toolchain_dir_in(base: &Path) -> PathBuf {
    let dir = base.join("toolchain");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Directory for downloaded QEMU builds **inside** `base` (created if missing).
///
/// The same shape as [`toolchain_dir_in`]: one directory per resource, one
/// versioned subdirectory inside it, and nothing pruned (F1).
pub fn qemu_dir_in(base: &Path) -> PathBuf {
    let dir = base.join("qemu");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Path of the sessions database **inside** `base`.
pub fn sessions_db_path_in(base: &Path) -> PathBuf {
    base.join("sessions.db")
}

/// Path of the local settings file.
///
/// Resolution order:
/// 1. `RISCDOM_SETTINGS_PATH` (tests / overrides)
/// 2. the registered app-data directory (`settings.json`)
/// 3. fallback: `<temp>/riscdom/settings.json`
pub fn settings_path() -> PathBuf {
    env_or_default("RISCDOM_SETTINGS_PATH", settings_path_in)
}

/// Directory for downloaded toolchains (`<app data>/toolchain`).
///
/// Resolution order:
/// 1. `RISCDOM_TOOLCHAIN_DIR` (tests / overrides)
/// 2. the registered app-data directory (`toolchain`)
/// 3. fallback: `<temp>/riscdom/toolchain`
pub fn toolchain_dir() -> PathBuf {
    env_or_default("RISCDOM_TOOLCHAIN_DIR", toolchain_dir_in)
}

/// Path of the sessions database.
///
/// Resolution order:
/// 1. `RISCDOM_SESSION_DB_PATH` (tests / overrides)
/// 2. the registered app-data directory (`sessions.db`)
/// 3. fallback: `<temp>/riscdom/sessions.db`
pub fn sessions_db_path() -> PathBuf {
    env_or_default("RISCDOM_SESSION_DB_PATH", sessions_db_path_in)
}
