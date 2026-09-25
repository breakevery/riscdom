//! OS keyring backend for persisting API keys.
//!
//! Keys live in the OS credential store (Windows Credential Manager / macOS
//! Keychain / Linux Secret Service). **Failures are silent**: callers degrade
//! to in-memory-only storage, never panic, and never block startup.
//!
//! `agent` does **not** depend on this module or on the `keyring` crate.

use std::collections::HashMap;
use std::sync::Mutex;

/// Service name used for all entries.
pub const SERVICE: &str = "com.breakevery.riscdom";

/// Keyring account name for a provider, e.g. `"llm-api-key:deepseek"`.
pub fn user_for_provider(provider_id: &str) -> String {
    format!("llm-api-key:{provider_id}")
}

/// Keyring account name for an in-network server's token, e.g.
/// `"remote-token:192.168.1.10:7821"` (v0.9.9 内网接入 4/N).
///
/// Keyed by the address the operator typed, so two servers are two entries and
/// connecting to a second node does not overwrite the first one's token. It is a
/// **credential**, which is why it is here and not in `settings.json` — the same
/// reason the provider key above is.
pub fn user_for_remote(host: &str) -> String {
    format!("remote-token:{host}")
}

/// A minimal keyring abstraction so tests can swap out the OS store.
pub trait KeyringBackend: Send + Sync {
    fn set(&self, service: &str, user: &str, password: &str) -> Result<(), String>;
    /// Returns `Ok(None)` when the entry does not exist (not an error).
    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String>;
    fn delete(&self, service: &str, user: &str) -> Result<(), String>;
}

/// The real OS keyring.
pub struct OsKeyring;

impl OsKeyring {
    pub fn new() -> Self {
        Self
    }
}

impl Default for OsKeyring {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringBackend for OsKeyring {
    fn set(&self, service: &str, user: &str, password: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(service, user).map_err(|e| e.to_string())?;
        entry.set_password(password).map_err(|e| e.to_string())
    }

    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(service, user).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            // Absent is not an error.
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(service, user).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            // Deleting a missing entry is fine.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// In-memory backend for tests (never touches the real OS keyring).
#[derive(Default)]
pub struct InMemoryKeyring {
    store: Mutex<HashMap<(String, String), String>>,
}

impl InMemoryKeyring {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyringBackend for InMemoryKeyring {
    fn set(&self, service: &str, user: &str, password: &str) -> Result<(), String> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| "keyring store poisoned".to_string())?;
        store.insert(
            (service.to_string(), user.to_string()),
            password.to_string(),
        );
        Ok(())
    }

    fn get(&self, service: &str, user: &str) -> Result<Option<String>, String> {
        let store = self
            .store
            .lock()
            .map_err(|_| "keyring store poisoned".to_string())?;
        Ok(store.get(&(service.to_string(), user.to_string())).cloned())
    }

    fn delete(&self, service: &str, user: &str) -> Result<(), String> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| "keyring store poisoned".to_string())?;
        store.remove(&(service.to_string(), user.to_string()));
        Ok(())
    }
}
