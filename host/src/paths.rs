//! Filesystem paths used by the host.
//!
//! Session data lives in the OS app-data directory -- never inside the repo and
//! never inside the AI workspace.

use std::path::PathBuf;
use std::sync::OnceLock;

static APP_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Register the application data directory (called from the Tauri setup).
///
/// Ignored if already set, so it is safe to call more than once.
pub fn set_app_data_dir(dir: PathBuf) {
    let _ = APP_DATA_DIR.set(dir);
}

/// The application data directory, if registered.
pub fn app_data_dir() -> Option<PathBuf> {
    APP_DATA_DIR.get().cloned()
}

/// Path of the local settings file.
///
/// Resolution order:
/// 1. `RISCDOM_SETTINGS_PATH` (tests / overrides)
/// 2. the registered app-data directory (`settings.json`)
/// 3. fallback: `<temp>/riscdom/settings.json`
pub fn settings_path() -> PathBuf {
    if let Ok(path) = std::env::var("RISCDOM_SETTINGS_PATH") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    let base = app_data_dir().unwrap_or_else(|| std::env::temp_dir().join("riscdom"));
    base.join("settings.json")
}

/// Path of the sessions database.
///
/// Resolution order:
/// 1. `RISCDOM_SESSION_DB_PATH` (tests / overrides)
/// 2. the registered app-data directory (`sessions.db`)
/// 3. fallback: `<temp>/riscdom/sessions.db`
pub fn sessions_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("RISCDOM_SESSION_DB_PATH") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    let base = app_data_dir().unwrap_or_else(|| std::env::temp_dir().join("riscdom"));
    base.join("sessions.db")
}
