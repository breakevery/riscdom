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
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            toolchain_path: None,
            qemu_path: None,
            preflight: None,
            theme: None,
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
